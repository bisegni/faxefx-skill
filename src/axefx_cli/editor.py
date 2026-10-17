"""Read-only, bounded import of installed editor evidence. Never opens MIDI."""
import collections
import hashlib
import plistlib
import re
import struct
import subprocess
from pathlib import Path

from .errors import AxeError, require

ANALYZED_SHA256 = '9a5b86e6e55b0bc8b64b44d1818434fa4e67fa023e5d0f4e1cb7e74eba741dd4'
BUILDER = '0x10026aba4'


def app_identity(app):
    root = Path(app).expanduser().resolve()
    info = plistlib.loads((root / 'Contents/Info.plist').read_bytes())
    executable = (root / 'Contents/MacOS' / info['CFBundleExecutable']).resolve()
    require(executable.is_relative_to(root), 'INVALID_EDITOR', 'Executable escapes application bundle')
    data = executable.read_bytes()
    require(len(data) <= 256 * 1024 * 1024, 'INVALID_EDITOR', 'Editor executable exceeds import limit')
    return data, {'app': str(root), 'executable': str(executable),
                  'version': info.get('CFBundleShortVersionString'),
                  'sha256': hashlib.sha256(data).hexdigest()}


def sections(data):
    """Decode just the ARM64 Mach-O slice/section table, with source bounds."""
    require(len(data) >= 32, 'INVALID_EDITOR', 'Truncated Mach-O header')
    if data[:4] == b'\xca\xfe\xba\xbe':
        count = struct.unpack_from('>I', data, 4)[0]
        require(0 < count <= 32 and 8 + count * 20 <= len(data), 'INVALID_EDITOR', 'Invalid fat architecture count')
        slices = [struct.unpack_from('>IIIII', data, 8 + i * 20) for i in range(count)]
        match = next((s for s in slices if s[0] == 0x100000c), None)
        require(match is not None, 'UNSUPPORTED_EDITOR', 'No ARM64 slice')
        _, _, offset, size, _ = match
        require(offset + size <= len(data), 'INVALID_EDITOR', 'Truncated ARM64 slice')
        data = data[offset:offset + size]
    require(data[:4] == b'\xcf\xfa\xed\xfe', 'UNSUPPORTED_EDITOR', 'Expected little-endian 64-bit Mach-O')
    count, command_bytes = struct.unpack_from('<II', data, 16)
    require(count <= 4096 and 32 + command_bytes <= len(data), 'INVALID_EDITOR', 'Invalid load commands')
    offset = 32
    result = []
    for _ in range(count):
        cmd, size = struct.unpack_from('<II', data, offset)
        require(size >= 8 and offset + size <= 32 + command_bytes, 'INVALID_EDITOR', 'Invalid command size')
        if cmd == 0x19:
            require(size >= 72, 'INVALID_EDITOR', 'Truncated segment')
            nsections = struct.unpack_from('<I', data, offset + 64)[0]
            require(72 + nsections * 80 <= size, 'INVALID_EDITOR', 'Invalid section count')
            for i in range(nsections):
                name, segment, address, length, file_offset = struct.unpack_from('<16s16sQQI', data, offset + 72 + 80 * i)
                result.append({'name': name.rstrip(b'\0').decode('ascii'),
                               'segment': segment.rstrip(b'\0').decode('ascii'),
                               'address': address, 'size': length, 'offset': file_offset})
        offset += size
    return data, result


def symbolic_ids(data):
    data, table = sections(data)
    strings = next((s for s in table if s['name'] == '__cstring'), None)
    require(strings is not None, 'UNSUPPORTED_EDITOR', 'Missing string section')
    values = collections.defaultdict(set)
    for section in table:
        if section['segment'] != '__DATA_CONST':
            continue
        require(section['offset'] + section['size'] <= len(data), 'INVALID_EDITOR', 'Truncated constant section')
        for pos in range(section['offset'], section['offset'] + section['size'] - 15, 8):
            value, pointer = struct.unpack_from('<QQ', data, pos)
            if value > 65535 or not strings['address'] <= pointer < strings['address'] + strings['size']:
                continue
            start = strings['offset'] + pointer - strings['address']
            end = data.find(b'\0', start, min(start + 102, len(data)))
            if end < 0:
                continue
            name = data[start:end].decode('ascii', 'replace')
            if re.fullmatch(r'[A-Z][A-Z0-9_]{2,100}', name):
                values[name].add(value)
    return {'ids': {k: next(iter(v)) for k, v in sorted(values.items()) if len(v) == 1},
            'conflicts': {k: sorted(v) for k, v in sorted(values.items()) if len(v) > 1}}


def decode_cache(path):
    source = Path(path).expanduser().resolve()
    require(source.stat().st_size <= 16 * 1024 * 1024, 'INVALID_CACHE', 'Cache exceeds 16 MiB')
    data = source.read_bytes()
    offset = 0

    def take(fmt):
        nonlocal offset
        size = struct.calcsize(fmt)
        require(offset + size <= len(data), 'INVALID_CACHE', 'Truncated cache', offset=offset)
        value = struct.unpack_from(fmt, data, offset)
        offset += size
        return value

    major, minor, revision = take('<IIH')
    require(revision in (3, 4), 'UNSUPPORTED_CACHE', 'Only observed cache revisions 3 and 4 are supported')
    timestamp = take('<32s')[0].split(b'\0', 1)[0].decode('ascii', 'replace') if revision == 4 else None
    count, = take('<I')
    require(count <= 256, 'INVALID_CACHE', 'Invalid effect count')
    effects = []
    for _ in range(count):
        effect, nparams = take('<II')
        require(nparams < 4096, 'INVALID_CACHE', 'Invalid parameter count')
        params = []
        for _ in range(nparams):
            start = offset
            pid, flags, unknown, a, b, c, d, nlabels = take('<HHHffffI')
            require(nlabels < 4096, 'INVALID_CACHE', 'Invalid label count')
            labels = []
            for _ in range(nlabels):
                size, = take('<I')
                require(size < 65536 and offset + size <= len(data), 'INVALID_CACHE', 'Invalid label length')
                labels.append(data[offset:offset + size].decode('utf8', 'replace'))
                offset += size
            tail_flags, nextra = take('<HI')
            require(nextra < 65536, 'INVALID_CACHE', 'Invalid extra value count')
            extra = list(take('<' + 'I' * nextra)) if nextra else []
            # Keep neutral names: these four floats are not validated conversion rules.
            import math
            params.append({'id': pid, 'offset': start, 'flags': flags, 'unknown16': unknown,
                           'numeric_fields': [v if math.isfinite(v) else None for v in (a, b, c, d)],
                           'labels': labels, 'tail_flags': tail_flags, 'extra_values': extra,
                           'editable_semantics_verified': False, 'special_record': pid >= 0xfff0})
        require(len({p['id'] for p in params}) == len(params), 'INVALID_CACHE', 'Duplicate parameter IDs')
        effects.append({'type_id': effect, 'parameters': params})
    require(offset == len(data), 'UNSUPPORTED_CACHE', 'Unparsed trailing cache data', parsed=offset, total=len(data))
    require(len({e['type_id'] for e in effects}) == len(effects), 'INVALID_CACHE', 'Duplicate effect types')
    return {'source': str(source), 'sha256': hashlib.sha256(data).hexdigest(),
            'firmware': {'major': major, 'minor': minor}, 'cache_revision': revision,
            'timestamp': timestamp, 'parsed_bytes': offset, 'effects': effects}


def import_editor(app, cache):
    data, identity = app_identity(app)
    definitions = decode_cache(cache)
    symbols = symbolic_ids(data)
    return {'schema_version': '1.0', 'kind': 'editor_catalog', 'editor': identity,
            'definitions': definitions, 'symbols': symbols,
            'hardware_schema_verified': False, 'device_binding': None,
            'warning': 'Local static evidence only. Firmware identity, instance limits, labels-to-parameter links and conversions require validation.'}


def inspect_editor(app):
    data, identity = app_identity(app)
    result = {'schema_version': '1.0', 'editor': identity,
              'known_build': identity['sha256'] == ANALYZED_SHA256,
              'symbolic_id_count': len(symbolic_ids(data)['ids']),
              'builder_address': None, 'direct_builder_call_sites': [],
              'coverage_complete': False, 'unresolved': ['indirect callers', 'operation semantics', 'receive dispatchers', 'hardware validation']}
    if not result['known_build']:
        result['unresolved'].append('Builder address must be recovered for this executable digest')
        return result
    result['builder_address'] = BUILDER
    try:
        process = subprocess.run(['otool', '-arch', 'arm64', '-tv', identity['executable']],
                                 capture_output=True, text=True, timeout=60, check=True)
    except (FileNotFoundError, subprocess.SubprocessError) as exc:
        raise AxeError('ANALYSIS_UNAVAILABLE', 'Read-only ARM64 inspection requires working macOS otool', reason=str(exc)) from exc
    result['direct_builder_call_sites'] = re.findall(r'^([0-9a-f]{16})\s+bl\s+0x10026aba4\b', process.stdout, re.M)
    lines = process.stdout.splitlines()
    result['call_site_inventory'] = [
        {'address': line.split()[0], 'disposition': 'builder caller identified; semantic classification unresolved',
         'preceding_instructions': lines[max(0, i - 24):i], 'hardware_validated': False}
        for i, line in enumerate(lines) if re.search(r'\bbl\s+0x10026aba4\b', line)]
    result['direct_call_site_count'] = len(result['direct_builder_call_sites'])
    result['note'] = 'Call sites are not distinct operations; names and disassembly do not prove device acceptance.'
    return result
