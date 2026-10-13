"""Small authored mock definitions plus observed numeric interoperability IDs.

No vendor cache, UI text, model list, or extracted symbol table is bundled.
Only the eight amp parameter IDs have implemented USB getter semantics; six
have the recovered setter path. All other parameter definitions are simulation
choices, not a hardware schema. Mock model names do not identify device models.
"""


def _parameter(identifier, name, low=0, high=1, options=(), observed=False):
    return {'id': identifier, 'name': name, 'symbols': [],
            'label': name.replace('-', ' ').title(),
            'numeric_fields': [low, high, 1, 0], 'options': list(options),
            'semantic_status': 'observed_id_mock_bounds' if observed else 'mock_definition'}


def _type(identifier, alias, name, instance_ids, parameters):
    instances = []
    for number, instance_id in enumerate(instance_ids, 1):
        ref = f'{alias}{number}'
        if alias.startswith(('input-', 'output-')):
            ref = alias.replace('-', '')
            if alias == 'input-usb':
                ref = 'input-usb'
        instances.append({'ref': ref, 'id': instance_id})
    return {'id': identifier, 'name': name, 'alias': alias,
            'instances': instances, 'parameters': parameters}


_amp = [_parameter(0, 'model', options=('Mock Clean', 'Mock Crunch', 'Mock Lead'), observed=True)]
_amp += [_parameter(identifier, name, observed=True) for identifier, name in
         ((1, 'gain'), (2, 'bass'), (3, 'mid'), (4, 'treble'), (5, 'master'), (20, 'presence'))]
_amp += [_parameter(21, 'level', -60, 20, observed=True)]

CATALOG = {
    'schema_version': '1.0',
    'catalog_id': 'minimal-mock-v1',
    'firmware': None,
    'source': 'Authored mock definitions and a limited set of observed numeric protocol IDs; not a firmware schema',
    'types': [
        _type(6, 'shunt', 'Shunt', [1024], []),
        _type(7, 'compressor', 'Compressor', range(46, 50), [
            _parameter(1, 'ratio', 1, 20), _parameter(11, 'mix')]),
        _type(10, 'amp', 'Amplifier', range(58, 62), _amp),
        _type(11, 'cab', 'Cabinet', range(62, 66), [_parameter(0, 'level', -60, 20)]),
        _type(12, 'reverb', 'Reverb', range(66, 70), [
            _parameter(0, 'type', options=('Mock Room', 'Mock Hall')),
            _parameter(1, 'time', .1, 30), _parameter(5, 'size', 0, 100),
            _parameter(13, 'mix'), _parameter(19, 'hicut', 20, 20000)]),
        _type(13, 'delay', 'Delay', range(70, 74), [
            _parameter(0, 'model', options=('Mock Mono', 'Mock Stereo')),
            _parameter(2, 'time', .001, 16), _parameter(4, 'feed', -1, 1),
            _parameter(17, 'mix'), _parameter(11, 'hicut', 20, 20000)]),
        _type(16, 'chorus', 'Chorus', range(78, 82), [
            _parameter(2, 'rate', .01, 20), _parameter(4, 'depth'), _parameter(10, 'mix')]),
        _type(25, 'drive', 'Drive', range(118, 122), [
            _parameter(1, 'drive'), _parameter(3, 'level', -60, 20), _parameter(4, 'mix')]),
        *[_type(40+n, f'input-{n}', f'Input {n}', [36+n], [_parameter(0, 'level', -60, 20)])
          for n in range(1, 5)],
        _type(45, 'input-usb', 'Input USB', [41], [_parameter(0, 'level', -60, 20)]),
        *[_type(45+n, f'output-{n}', f'Output {n}', [41+n], [_parameter(0, 'level', -60, 20)])
          for n in range(1, 5)],
        # Recognized identity, deliberately unavailable for mock insertion.
        _type(60, 'nam-player', 'NAM Player', range(206, 210), []),
    ],
}
