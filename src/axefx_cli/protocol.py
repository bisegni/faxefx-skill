"""Evidence-bounded protocol inventory; this registry never enables writes."""
import copy

PUBLIC_SOURCE = 'https://fractalaudio.com/downloads/misc/Axe-Fx%20III%20MIDI%20for%203rd%20Party%20Devices.pdf'


def coverage():
    rows = []
    public = [
        ('0A', 'Effect bypass', True, True, 'amp1 fixed query only'),
        ('0B', 'Effect channel', True, True, 'amp1 fixed query only'),
        ('0C', 'Scene', True, True, 'current-scene query'),
        ('0D', 'Preset name', True, False, 'current-preset query'),
        ('0E', 'Scene name', True, False, 'current-scene query'),
        ('0F', 'Looper', False, False, 'outside preset authoring scope'),
        ('10', 'Tap tempo', False, False, 'outside preset authoring scope'),
        ('11', 'Tuner', False, False, 'outside preset authoring scope'),
        ('13', 'Effect status', True, False, 'current-preset query'),
        ('14', 'Tempo', True, True, 'query only'),
    ]
    for opcode, name, implemented_read, missing_set, scope in public:
        rows.append({'id': 'public-' + opcode, 'name': name, 'outer_opcode': opcode,
                     'documented': True, 'statically_recovered': False,
                     'read_implemented': implemented_read, 'write_implemented': False,
                     'hardware_validation': 'not established here; fixed query evidence only' if implemented_read else 'unverified',
                     'scope': scope, 'missing_setter': missing_set, 'source': PUBLIC_SOURCE,
                     'frame': 'manufacturer 000174; model 10; 7-bit data; XOR checksum',
                     'retry_policy': 'read-only bounded query; no mutating retry contract'})
    rows.extend([
        {'id': 'editor-001A', 'name': 'Active amp parameter getter', 'outer_opcode': '01', 'inner_operation': '001A',
         'documented': False, 'statically_recovered': True, 'read_implemented': True, 'write_implemented': False,
         'hardware_validation': 'historical bounded active-amp getter validation; current firmware unidentified',
         'scope': 'eight amp parameters, active channel only', 'reply': 'matching operation/effect/parameter, index=0, float bits and optional display text'},
        {'id': 'editor-0009', 'name': 'Numeric amp setter', 'outer_opcode': '01', 'inner_operation': '0009',
         'documented': False, 'statically_recovered': True, 'read_implemented': False, 'write_implemented': True,
         'hardware_validation': 'unverified', 'scope': 'gain/bass/mid/treble/master/presence; explicit approved preview and readback',
         'retry_policy': 'never blindly retry; inspect journal and captured parameter restoration'},
        {'id': 'editor-002E', 'name': 'Edit-buffer physical grid', 'outer_opcode': '01', 'inner_operation': '002E',
         'documented': False, 'statically_recovered': True, 'read_implemented': True, 'write_implemented': False,
         'hardware_validation': 'historical read-only topology validation; not a full-preset snapshot',
         'reply': '640 unpacked bytes, 84 column-major cells at offset 304; repeated grid and context checks'},
    ])
    for name, opcodes in [('block-transfer', ['74', '75', '76']), ('preset-transfer', ['77', '78', '79'])]:
        rows.append({'id': name, 'name': name, 'outer_opcodes': opcodes, 'documented': False,
                     'statically_recovered': True, 'read_implemented': False, 'write_implemented': False,
                     'hardware_validation': 'unverified', 'scope': 'framing observed in saved files; payload, destination, acknowledgement and pacing unresolved'})
    for name in ['device/firmware identity', 'full edit-buffer backup/restore', 'block insertion/removal/movement',
                 'cable editing', 'model selection and parameter conversions', 'scene contents', 'inactive channels',
                 'modifiers/controllers', 'preset save/reload']:
        rows.append({'id': 'gap-' + name, 'name': name, 'documented': False, 'statically_recovered': False,
                     'read_implemented': False, 'write_implemented': False, 'hardware_validation': 'unverified',
                     'scope': 'required for full automatic USB authoring'})
    rows.append({'id': 'push-data', 'name': 'Unsolicited tempo/tuner', 'documented': True,
                 'read_implemented': False, 'write_implemented': False, 'source': PUBLIC_SOURCE,
                 'offline_decoder': True,
                 'frame': 'checksumless exceptions; documented MIDI DIN output only, current transport behavior unverified'})
    rows.append({'id': 'standard-midi', 'name': 'PC / assignable CC / realtime MIDI', 'documented': True,
                 'read_implemented': False, 'write_implemented': False, 'source': PUBLIC_SOURCE,
                 'scope': 'device mappings must be discovered; no assumed fixed CC assignments'})
    for row in rows:
        if row['id'].startswith('public-'):
            row['offline_decoder'] = True
    return {'schema_version': '1.0', 'complete': False, 'private_protocol_complete': False,
            'entries': copy.deepcopy(rows), 'public_fixed_query_templates': 7,
            'editor_envelope': {'outer_opcode': '01', 'header_bytes': 15, 'integer_encoding': 'low-first 7-bit groups',
                                'binary_encoding': 'MSB-first bitstream packed to seven bits, zero-padded'},
            'hardware_gate': 'Full structural authoring requires validated firmware identity, complete backup/restore, handlers and readback.',
            'warning': 'Coverage is evidence, not permission. Static recovery and mock tests do not establish hardware validation.'}
