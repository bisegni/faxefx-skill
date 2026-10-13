"""Preview/approve/apply workflow, immutable plans and failure journals."""
import copy
import time

from .errors import AxeError, require
from .operations import apply_operation, validate_operation
from .storage import digest, load


class Engine:
    def __init__(self,backend,store):
        self.backend=backend;self.store=store

    def snapshot(self):
        require(self.backend.capabilities()['complete_snapshot'],'PROTOCOL_UNRESOLVED',
                'Complete edit-buffer dump/restore is unresolved on this backend; parameter snapshots are captured automatically for numeric edits')
        state=self.backend.read_state()
        artifact={'schema_version':'1.0','backend':self.backend.name,'identity':self.backend.identity,
                  'scope':'edit-buffer','state':state}
        return {**self.store.put('snapshots',artifact),'scope':artifact['scope']}

    def normalize(self,operations):
        require(isinstance(operations,list) and 1<=len(operations)<=256,'INVALID_PLAN','Provide 1–256 typed operations')
        snapshots={};expanded=[]
        for op in operations:
            validate_operation(op)
            if op['op']=='snapshot.restore':
                snap=self.store.get('snapshots',op['snapshot_id'])
                require(snap['backend']==self.backend.name and snap['identity']==self.backend.identity,
                        'DEVICE_CHANGED','Snapshot belongs to a different backend/device')
                snapshots[op['snapshot_id']]=snap
                if self.backend.name!='mock':
                    require('source_slot' not in op,'PROTOCOL_UNRESOLVED','Stored-slot backup restore is unresolved over USB')
                    require(snap['scope']=='parameters','PROTOCOL_UNRESOLVED','Full USB preset restore is unresolved')
                    for b in snap['state']['current']['blocks']:
                        channel=b['active_channel']
                        for param,value in b['channels'][channel]['parameters'].items():
                            expanded.append({'op':'parameter.set','block':b['ref'],'parameter':param,'value':value['raw_value'],'raw':True,'channel':channel})
                    continue
            expanded.append(copy.deepcopy(op))
        require(bool(expanded),'INVALID_PLAN','No operations to apply')
        require(not any(o['op']=='preset.save' for o in expanded) or len(expanded)==1,'SAVE_SEPARATE',
                'Saving needs a separate preview and approval after auditioning')
        return expanded,snapshots

    def plan(self,operations):
        operations,snapshots=self.normalize(operations)
        self.backend.check_operations(operations)
        original=self.backend.read_state(operations)
        predicted,notes=self.backend.simulate(original,operations,snapshots)
        plan={'schema_version':'1.0','backend':self.backend.name,'identity':self.backend.identity,
              'created_at':time.time(),'base_fingerprint':digest(original),'operations':operations,
              'snapshot_refs':sorted(snapshots),'before':original,'after':predicted,'notes':notes,
              'verification':self.backend.capabilities()['verification']}
        artifact=self.store.put('plans',plan)
        return {**artifact,'approval_required':True,'operations':operations,'notes':notes,
                'before':original['current'],'after':predicted['current'],
                'save_destination':operations[0]['slot'] if operations[0]['op']=='preset.save' else None,
                'destination_backup':original['presets'].get(str(operations[0]['slot'])) if operations[0]['op']=='preset.save' and original.get('presets') else None,
                'verification':plan['verification']}

    def apply(self,key,approved=False):
        # Approval is an acknowledgement by the caller, not cryptographic proof of a user event.
        require(approved,'APPROVAL_REQUIRED','The agent must obtain approval for this exact preview, then pass --approved',plan_id=key)
        plan=self.store.get('plans',key)
        require(plan['backend']==self.backend.name and plan['identity']==self.backend.identity,
                'DEVICE_CHANGED','Plan belongs to a different backend/device')
        require(self.backend.capabilities()['writes_enabled'],'READ_ONLY_BACKEND','This backend cannot apply edits')
        journal_path=self.store.root/'journals'/(key+'.json')
        require(not journal_path.exists(),'PLAN_ALREADY_ATTEMPTED','Plan was already attempted; inspect its journal and create a fresh plan',journal=str(journal_path))
        operations=plan['operations'];self.backend.check_operations(operations)
        original=self.backend.read_state(operations)
        require(digest(original)==plan['base_fingerprint'],'STALE_PLAN','Device state changed since preview; create and approve a new plan')
        snapshots={k:self.store.get('snapshots',k) for k in plan['snapshot_refs']}
        for snapshot in snapshots.values():
            require(snapshot['backend']==plan['backend'] and snapshot['identity']==plan['identity'],
                    'DEVICE_CHANGED','Snapshot belongs to another backend/device')
        # Revalidate executable operations, not just the approved explanatory output.
        predicted,_=self.backend.simulate(original,operations,snapshots)
        require(predicted==plan['after'],'PLAN_INVALID','Plan prediction does not match current operation semantics')
        snapshot=self.store.put('snapshots',{'schema_version':'1.0','backend':self.backend.name,'identity':self.backend.identity,
                                            'scope':'edit-buffer' if self.backend.name=='mock' else 'parameters','state':original})
        journal={'plan_id':key,'status':'applying','snapshot':snapshot,'completed':[], 'attempted':None,
                 'restore_status':None,'approval_acknowledged':True}
        self.store.journal(key,journal)
        expected=copy.deepcopy(original)
        try:
            for index,op in enumerate(operations):
                journal['attempted']=index;self.store.journal(key,journal)
                if hasattr(self.backend,'guard_state'):
                    self.backend.guard_state=copy.deepcopy(expected);self.backend.guard_operations=operations
                self.backend.apply(op,snapshots)
                expected,_=self.backend.simulate(expected,[op],snapshots)
                actual=self.backend.read_state(operations)
                require(self.backend.equivalent(expected,actual),'READBACK_MISMATCH','Observed state differs from planned state',operation_index=index)
                journal['completed'].append(index);self.store.journal(key,journal)
            journal['status']='applied';journal['attempted']=None;self.store.journal(key,journal)
            return {'plan_id':key,'status':'applied','applied_operations':len(operations),'snapshot':snapshot,
                    'verification':self.backend.capabilities()['verification'],'current':actual['current']}
        except (Exception, KeyboardInterrupt) as exc:
            journal['status']='failed';journal['error']=str(exc)
            try:
                require(getattr(exc,'code',None) not in ('CONTEXT_CHANGED','DEVICE_CHANGED'),
                        'CONTEXT_CHANGED','Automatic restoration skipped after an external context change; preview recovery against current state')
                self.backend.restore(original,operations)
                restored=self.backend.read_state(operations)
                require(self.backend.equivalent(original,restored),'RESTORE_MISMATCH','Original state was not fully restored')
                journal['restore_status']='verified'
            except Exception as restore_error:
                journal['restore_status']='failed_or_unresolved';journal['restore_error']=str(restore_error)
            self.store.journal(key,journal)
            raise AxeError('APPLY_FAILED','Application failed; consult restoration result before further edits',
                           cause_code=getattr(exc,'code','INTERNAL_ERROR'),journal=str(journal_path),
                           completed=journal['completed'],attempted=journal['attempted'],
                           snapshot=snapshot,restore_status=journal['restore_status'],
                           reason=str(exc),restore_error=journal.get('restore_error')) from exc
        finally:
            if hasattr(self.backend,'guard_state'):
                self.backend.guard_state=None;self.backend.guard_operations=()
