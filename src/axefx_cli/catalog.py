"""Authored mock catalog with a limited observed USB parameter contract."""
import copy
import math

from .definitions import CATALOG
from .errors import AxeError, require

SCALED_AMP = {'gain', 'bass', 'mid', 'treble', 'master', 'presence'}
READ_AMP = SCALED_AMP | {'model', 'level'}


def effect_type(value):
    aliases = {'amplifier': 'amp', 'cabinet': 'cab', 'input': 'input-1', 'output': 'output-1'}
    value = aliases.get(str(value).lower(), str(value).lower())
    matches = [t for t in CATALOG['types'] if value in (t['alias'], str(t['id']), t['name'].lower())]
    require(len(matches) == 1, 'UNKNOWN_BLOCK_TYPE', 'Choose a type from catalog', type=value)
    return matches[0]


def instance_ref(type_, instance):
    # Input/output have separate type IDs per physical socket.
    if type_['alias'].startswith(('input-', 'output-')):
        return instance['ref']
    return instance['ref']


def type_for_instance(identifier):
    for t in CATALOG['types']:
        for i in t['instances']:
            if i['id'] == identifier:
                return t, i
    raise AxeError('UNKNOWN_BLOCK_TYPE', 'Block ID is absent from the static catalog', instance_id=identifier)


def parameter(type_, value):
    value = str(value)
    matches = [p for p in type_['parameters'] if value.lower() == p['name'] or value == str(p['id'])
               or value.upper() in p['symbols']]
    if not matches:
        matches = [p for p in type_['parameters'] if value.lower() == p['label'].lower()]
    require(len(matches) == 1, 'UNKNOWN_PARAMETER', 'Choose an unambiguous parameter from block inspect', parameter=value)
    return copy.deepcopy(matches[0])


def finite(value):
    require(isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value),
            'INVALID_VALUE', 'Expected a finite numeric value', value=repr(value))
    return float(value)


def mock_value(type_, param, value, raw=False):
    """Authored simulation bounds, explicitly not proof of device conversion."""
    p = parameter(type_, param)
    if p['options']:
        require(not raw, 'INVALID_VALUE', 'Choose an option by name for simulated enumerations')
        require(value in p['options'], 'INVALID_VALUE', 'Value is not a catalog option', options=p['options'])
        return {'value': value, 'raw_value': None, 'display': value, 'unit': 'option', 'semantics': 'simulated'}
    v = finite(value)
    low, high, scale, _ = p['numeric_fields']
    if type_['alias'] == 'amp' and p['name'] in SCALED_AMP:
        low, high, scale = 0, 1, 10
    else:
        scale = 1  # Simulation units do not establish hardware conversion.
    if raw:
        require(type_['alias'] == 'amp' and p['name'] in SCALED_AMP, 'CONVERSION_UNRESOLVED',
                'Raw conversion is only implemented for tested amp knobs')
        v *= scale
    require(low * scale <= v <= high * scale, 'INVALID_VALUE', 'Outside simulated catalog bounds', minimum=low*scale, maximum=high*scale)
    return {'value': v, 'raw_value': v / scale if p['name'] in SCALED_AMP and type_['alias'] == 'amp' else None,
            'display': f'{v:.2f}', 'unit': 'knob' if scale == 10 else 'mock', 'semantics': 'simulated'}


def summary(type_=None):
    if type_ is None:
        return {'firmware': CATALOG['firmware'], 'catalog_id': CATALOG['catalog_id'], 'provenance': CATALOG['source'],
                'types': [{**{k: t[k] for k in ('id', 'name', 'alias')},
                           'instances': [{**i, 'ref': instance_ref(t, i)} for i in t['instances']],
                           'parameter_count': len(t['parameters'])} for t in CATALOG['types']]}
    result = copy.deepcopy(type_)
    result['firmware'] = CATALOG['firmware']
    result['catalog_id'] = CATALOG['catalog_id']
    result['provenance'] = CATALOG['source']
    result['hardware_schema_verified'] = False
    for p in result['parameters']:
        p['usb_get'] = type_['alias'] == 'amp' and p['name'] in READ_AMP
        p['usb_set'] = type_['alias'] == 'amp' and p['name'] in SCALED_AMP
        if p['usb_set']:
            p['unit'] = 'knob'
            p['display_range'] = [0, 10]
    return result
