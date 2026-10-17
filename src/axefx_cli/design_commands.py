"""Thin CLI adapters; browser and agent commands share these core services."""
from pathlib import Path

from .design import Designs, compile_design, from_capture, starter
from .editor import import_editor, inspect_editor
from .engine import Engine
from .errors import require
from .protocol import coverage
from .render import render_file
from .storage import load


def register(groups):
    ui = groups.add_parser('ui', help='Open the workbench and read the connected preset')
    ui.add_argument('--port', type=int, default=0)
    ui.add_argument('--no-browser', action='store_true')
    protocol = groups.add_parser('protocol').add_subparsers(dest='action', required=True)
    protocol.add_parser('coverage')
    command = protocol.add_parser('decode'); command.add_argument('--hex', required=True)
    command.add_argument('--direction', choices=('rx', 'tx'), default='rx')
    command = protocol.add_parser('inspect-editor'); command.add_argument('--app', required=True)
    root = groups.add_parser('design').add_subparsers(dest='action', required=True)
    for action in ('create', 'import', 'inspect', 'revise', 'compile', 'preview', 'render', 'open', 'capture'):
        c = root.add_parser(action)
        if action in ('create', 'import'):
            c.add_argument('--file', required=action == 'import'); c.add_argument('--name', default='Untitled')
        elif action != 'capture':
            c.add_argument('project'); c.add_argument('--revision')
        if action == 'revise':
            c.add_argument('--base', required=True); c.add_argument('--file', required=True)
        if action == 'render':
            c.add_argument('--output', required=True); c.add_argument('--physical', action='store_true'); c.add_argument('--block')
            c.add_argument('--scene', type=int)
        if action == 'open':
            c.add_argument('--port', type=int, default=0); c.add_argument('--no-browser', action='store_true')
    audio = groups.add_parser('audio').add_subparsers(dest='action', required=True)
    c = audio.add_parser('import'); c.add_argument('--file', required=True)
    c = audio.add_parser('analyze'); c.add_argument('--audio-id', required=True)
    c.add_argument('--start', type=float, default=0); c.add_argument('--duration', type=float, default=30)
    c.add_argument('--separate', action='store_true')
    c = audio.add_parser('inspect'); c.add_argument('id')
    c = audio.add_parser('compare'); c.add_argument('reference'); c.add_argument('recording')


def offline(a):
    return (a.group in ('protocol', 'audio', 'ui') or
            a.group == 'catalog' and getattr(a, 'action', None) is not None or
            a.group == 'design' and a.action not in ('preview', 'capture'))


def execute_local(a, store):
    if a.group == 'ui':
        from .workbench import serve
        return serve(store, None, a.backend, a.port, not a.no_browser,
                     input_name=a.input, output_name=a.output, replay_file=a.replay_file)
    if a.group == 'protocol':
        if a.action == 'coverage':
            return coverage()
        if a.action == 'decode':
            from .codec import decode_editor, decode_public, HEADER
            packet = bytes.fromhex(a.hex)
            return decode_editor(packet) if packet[:6] == HEADER + b'\x01' else decode_public(packet, a.direction)
        return inspect_editor(a.app)
    if a.group == 'catalog':
        if a.action == 'import-editor':
            require(a.app and a.cache, 'INVALID_ARGUMENTS', 'Supply --app and explicit --cache; cache selection never guesses device firmware')
            catalog = import_editor(a.app, a.cache)
            artifact = store.put('editor-catalogs', catalog)
            return {**artifact, 'editor': catalog['editor'], 'firmware': catalog['definitions']['firmware'],
                    'effect_count': len(catalog['definitions']['effects']),
                    'parameter_count': sum(len(e['parameters']) for e in catalog['definitions']['effects']),
                    'hardware_schema_verified': False, 'warning': catalog['warning']}
        require(a.id, 'INVALID_ARGUMENTS', 'Supply --id from catalog import-editor')
        catalog = store.get('editor-catalogs', a.id)
        require(a.type is not None or a.parameter is None, 'INVALID_ARGUMENTS', '--parameter requires --type')
        metadata = {'id': a.id, 'editor': catalog['editor'], 'firmware': catalog['definitions']['firmware'],
                    'hardware_schema_verified': False, 'warning': catalog['warning']}
        if a.symbol:
            require(a.type is None and a.parameter is None, 'INVALID_ARGUMENTS', '--symbol is independent of type/parameter queries')
            require(a.symbol in catalog['symbols']['ids'], 'UNKNOWN_SYMBOL', 'Symbol is absent or conflicted in this executable')
            return {**metadata, 'symbol': a.symbol, 'value': catalog['symbols']['ids'][a.symbol],
                    'type_binding_verified': False, 'note': 'Numeric enumeration only; no parameter-type or wire conversion binding is implied.'}
        if a.type is None:
            return {**metadata, 'symbol_count': len(catalog['symbols']['ids']),
                    'types': [{'id': e['type_id'], 'parameter_count': len(e['parameters'])} for e in catalog['definitions']['effects']]}
        effects = [e for e in catalog['definitions']['effects'] if str(e['type_id']) == a.type]
        require(len(effects) == 1, 'UNKNOWN_BLOCK_TYPE', 'Use a numeric type ID from this imported catalog')
        effect = effects[0]
        if a.parameter is None:
            return {**metadata, 'type_id': effect['type_id'], 'parameters': [
                {'id': p['id'], 'label_count': len(p['labels']), 'special_record': p['special_record'],
                 'editable_semantics_verified': p['editable_semantics_verified']} for p in effect['parameters']]}
        require(a.parameter.isdecimal(), 'SYMBOL_SCOPE_UNRESOLVED', 'Use a numeric parameter ID; imported symbols are not automatically bound to effect types. Inspect --symbol independently.')
        identifier = a.parameter
        parameters = [p for p in effect['parameters'] if str(p['id']) == str(identifier)]
        require(len(parameters) == 1, 'UNKNOWN_PARAMETER', 'Use a numeric parameter ID from this imported type')
        return {**metadata, 'type_id': effect['type_id'], 'parameter': parameters[0]}
    if a.group == 'audio':
        from .audio import analyze, compare, import_audio
        if a.action == 'import':
            return import_audio(store, a.file)
        if a.action == 'analyze':
            return analyze(store, a.audio_id, a.start, a.duration, a.separate)
        if a.action == 'inspect':
            return store.get('audio-analyses', a.id)
        return compare(store, a.reference, a.recording)
    designs = Designs(store)
    if a.action in ('create', 'import'):
        return designs.create(load(a.file) if a.file else starter(a.name))
    if a.action == 'revise':
        return designs.revise(a.project, a.base, load(a.file))
    artifact = designs.get(a.project, a.revision)
    if a.action == 'inspect':
        return artifact
    if a.action == 'compile':
        compiled = compile_design(artifact['design'], a.backend)
        compiled.update(project=a.project, revision=artifact['revision'])
        return {**compiled, **store.put('compilations', compiled)}
    if a.action == 'render':
        source = artifact['design']
        if a.physical:
            captured = source.get('captured')
            if captured:
                # Physical means the captured state; it must not masquerade as the edited draft.
                source = captured
                require(not a.block, 'INVALID_ARGUMENTS', 'Block cards use logical IDs; omit --physical for a block card')
            else:
                source = compile_design(source, a.backend)
        return render_file(source, a.output, a.physical and source.get('kind') == 'compiled_design', a.block, a.scene)
    if a.action == 'open':
        from .workbench import serve
        return serve(store, a.project, a.backend, a.port, not a.no_browser)
    raise ValueError('Unsupported local command')


def execute_device(a, backend, store):
    designs = Designs(store)
    if a.action == 'capture':
        state = backend.read_state(include_geometry=True) if backend.name != 'mock' else backend.read_state()
        return designs.create(from_capture(state))
    artifact = designs.get(a.project, a.revision)
    require(artifact['revision'] == artifact['head_revision'], 'REVISION_CONFLICT', 'Only the current revision can produce a hardware preview')
    compiled = compile_design(artifact['design'], backend.name, backend.capabilities())
    require(compiled['buildable'], 'DESIGN_BLOCKED', 'Resolve compilation blockers before requesting an executable preview', blockers=compiled['blockers'])
    if artifact['design'].get('mode') == 'refine':
        require(backend.equivalent(artifact['design']['captured'], backend.read_state()), 'STALE_CAPTURE', 'Device differs from design capture; capture again and merge the draft')
    return Engine(backend, store).plan(compiled['operations'], provenance={'project': a.project, 'revision': artifact['revision']})
