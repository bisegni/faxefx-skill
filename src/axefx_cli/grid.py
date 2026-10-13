"""Read-only decoding of the recovered Axe-Fx III edit-buffer grid.

Operation 0x2e returns 640 unpacked bytes. The last 336 bytes contain 84
column-major cells: little-endian uint16 block ID, then uint16 input row mask.
Keep the full unpacked IDs: reading markers directly from packed MIDI truncates
some shunt IDs. Unknown payload layouts fail rather than inventing routing.
"""
import struct

from .codec import decode_editor
from .errors import AxeError, require

ROWS = 6
COLUMNS = 14
CELL_OFFSET = 304
PAYLOAD_SIZE = CELL_OFFSET + ROWS * COLUMNS * 4


def decode_grid(reply):
    try:
        envelope = decode_editor(reply)
    except ValueError as exc:
        raise AxeError('INVALID_REPLY', 'Invalid grid envelope', reason=str(exc)) from exc
    require(envelope['operation'] == 0x2e and envelope['effect'] == 0 and
            envelope['parameter'] == 0 and envelope['index'] == 0,
            'INVALID_REPLY', 'Unexpected grid response fields')
    # value_bits is the editor cursor, not a placement or success flag.
    data = bytes.fromhex(envelope['data_hex'])
    require(len(data) == PAYLOAD_SIZE, 'PROTOCOL_UNRESOLVED',
            'Unsupported grid payload layout', expected_bytes=PAYLOAD_SIZE, actual_bytes=len(data))
    slot = struct.unpack_from('<I', data)[0]
    require(slot <= 1023, 'INVALID_REPLY', 'Grid preset slot outside supported range')
    records = []
    for column in range(1, COLUMNS + 1):
        for row in range(1, ROWS + 1):
            index = (column - 1) * ROWS + row - 1
            identifier, inputs = struct.unpack_from('<HH', data, CELL_OFFSET + index * 4)
            require(inputs < 1 << ROWS, 'PROTOCOL_UNRESOLVED',
                    'Unsupported grid input mask', row=row, column=column, input_mask=inputs)
            require(column > 1 or inputs == 0, 'INVALID_REPLY',
                    'First-column cell cannot reference a previous column', row=row)
            records.append({'row': row, 'column': column,
                            'instance_id': identifier, 'input_mask': inputs})
    return {'slot': slot,
            'name': data[16:48].split(b'\0', 1)[0].decode('ascii', 'replace').rstrip(),
            'geometry': {'rows': ROWS, 'columns': COLUMNS}, 'records': records}


def attach_grid(preset, grid):
    """Join status blocks with placements, retaining shunts and empty-cell cables."""
    require((preset['slot'], preset['name']) == (grid['slot'], grid['name']),
            'CONTEXT_CHANGED', 'Grid and public preset identity do not match')
    status = {b['instance_id']: b for b in preset['blocks']}
    require(len(status) == len(preset['blocks']), 'INVALID_REPLY', 'Duplicate block status ID')
    cells = {(r['row'], r['column']): r for r in grid['records']}
    occupied = set()
    refs = {}
    # Cables can bind to empty cells. Represent those endpoints explicitly;
    # they are wires, never guessed effect blocks or implicitly connected shunts.
    used = {(r['row'], r['column']) for r in grid['records'] if r['instance_id'] or r['input_mask']}
    for record in grid['records']:
        for source_row in range(1, ROWS + 1):
            if record['input_mask'] & (1 << (source_row - 1)):
                used.add((source_row, record['column'] - 1))
    for record in grid['records']:
        row, column, ident = record['row'], record['column'], record['instance_id']
        if (row, column) not in used:
            continue
        if ident and not 1024 <= ident < 1088:
            require(ident in status, 'CONTEXT_CHANGED',
                    'Placed block is missing from status', instance_id=ident, row=row, column=column)
            effect = status[ident]
        else:
            kind = 'shunt' if ident else 'wire'
            effect = {'ref': f'{kind}-r{row}c{column}', 'instance_id': ident or None,
                      'type_id': 6 if ident else None, 'type': kind,
                      'active_channel': None, 'bypass': None, 'channels': {}}
            preset['blocks'].append(effect)
        require(not ident or ident not in occupied, 'INVALID_REPLY',
                'Block instance appears in multiple grid cells', instance_id=ident)
        occupied.add(ident)
        effect['position'] = {'row': row, 'column': column}
        effect['input_mask'] = record['input_mask']
        refs[row, column] = effect['ref']
    connections = []
    for (row, column), target in refs.items():
        for source_row in range(1, ROWS + 1):
            if cells[row, column]['input_mask'] & (1 << (source_row - 1)):
                connections.append({'from': refs[source_row, column - 1], 'to': target})
    preset.update(geometry=grid['geometry'], connections=connections, complete_geometry=True)
