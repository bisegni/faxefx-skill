"""Deterministic diagrams from observed or proposed graph data, never ImageGen."""
from html import escape
from pathlib import Path

from .design import layout
from .errors import AxeError, require

COLORS = {'amp': '#b97435', 'cab': '#7a609e', 'drive': '#c5504c', 'compressor': '#ad8540',
          'delay': '#29858e', 'reverb': '#526caf', 'chorus': '#9c5d88', 'shunt': '#66788a', 'wire': '#66788a'}


def scene_value(source, scene, node, number, field):
    values = scene.get('bypass' if field == 'bypass' else 'channels', {})
    if node['id'] in values:
        return values[node['id']]
    captured = (source.get('captured') or {}).get('current', {})
    if captured and not captured.get('complete') and captured.get('active_scene') != number:
        return None
    return node.get(field)


def graph(source, physical=False, scene_number=None):
    if source.get('kind') == 'compiled_design':
        if physical:
            require(source.get('physical') is not None, 'GEOMETRY_UNAVAILABLE', 'No compiled hardware geometry is available')
            name, nodes, edges, _ = graph(source['physical'], scene_number=scene_number)
            return name, nodes, edges, 'SIMULATED compiled grid' if source['backend'] == 'mock' else 'PROPOSED compiled grid'
        source = source['logical']
    if 'design_version' in source:
        try:
            positions = layout(source)
        except AxeError:
            positions = {n['id']: {'row': i // 10 + 1, 'column': i % 10 + 1} for i, n in enumerate(source['nodes'])}
        number = scene_number or source.get('active_scene', 1)
        require(type(number) is int and 1 <= number <= 8, 'INVALID_SCENE', 'Scenes use 1–8')
        scene = next((s for s in source.get('scenes', []) if s['number'] == number), {})
        nodes = [{**n, 'position': positions[n['id']],
                  'bypass': scene_value(source, scene, n, number, 'bypass'),
                  'active_channel': scene_value(source, scene, n, number, 'active_channel')}
                 for n in source['nodes']]
        return source['name'], nodes, source['edges'], f'PROPOSED logical design · scene {number}'
    captured_label = None
    if 'current' in source:
        captured_label = 'SIMULATED capture' if source.get('device', {}).get('simulated') else 'CAPTURED physical grid'
        source = source['current']
    require('blocks' in source and source.get('connections') is not None and
            source.get('complete_geometry', source.get('complete', False)),
            'GEOMETRY_UNAVAILABLE', 'Verified geometry is required; connections cannot be guessed')
    nodes = [{**b, 'id': b['ref'], 'label': b['ref']} for b in source['blocks'] if b.get('position') is not None]
    if scene_number is not None:
        require(type(scene_number) is int and 1 <= scene_number <= 8, 'INVALID_SCENE', 'Scenes use 1–8')
        require(source.get('scenes') is not None or source.get('active_scene') == scene_number,
                'PROTOCOL_UNRESOLVED', 'This capture has no contents for the selected scene')
        scene = next((s for s in source.get('scenes') or [] if s['number'] == scene_number), {})
        nodes = [{**n, 'bypass': scene.get('bypass', {}).get(n['id'], n.get('bypass')),
                  'active_channel': scene.get('channels', {}).get(n['id'], n.get('active_channel'))} for n in nodes]
    label = captured_label or 'PROPOSED physical grid'
    if not source.get('complete'):
        label += ' · partial settings'
    return source['name'], nodes, source['connections'], label


def svg(source, physical=False, block_id=None, scene_number=None):
    name, nodes, edges, status = graph(source, physical, scene_number)
    if block_id:
        matches = [n for n in nodes if n['id'] == block_id]
        require(len(matches) == 1, 'UNKNOWN_BLOCK', 'Choose a block ID from this design')
        return block_svg(name, matches[0], status)
    columns = max((n['position']['column'] for n in nodes), default=4)
    rows = max(6, max((n['position']['row'] for n in nodes), default=1))
    width, height = max(900, columns * 168 + 80), rows * 104 + 170
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="{escape(name, quote=True)}">',
             '<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10Z" fill="#577285"/></marker></defs>',
             f'<rect width="{width}" height="{height}" rx="16" fill="#edf2f6"/>',
             f'<text x="32" y="39" font-family="Helvetica,Arial,sans-serif" font-size="23" font-weight="600" fill="#20364c">{escape(name)}</text>',
             f'<text x="32" y="65" font-family="Helvetica,Arial,sans-serif" font-size="13" fill="#52687b">{escape(status)}</text>']
    coords = {}
    for n in nodes:
        coords[n['id']] = (40 + (n['position']['column'] - 1) * 168, 100 + (n['position']['row'] - 1) * 104)
    for c in range(1, columns + 1):
        parts.append(f'<text x="{100 + (c-1)*168}" y="91" font-family="Helvetica,Arial,sans-serif" font-size="11" fill="#718395">{c}</text>')
    for e in edges:
        if e['from'] not in coords or e['to'] not in coords:
            continue
        x, y = coords[e['from']]; a, b = coords[e['to']]; middle = (x + 140 + a) / 2
        parts.append(f'<path d="M{x+140} {y+32} H{middle} V{b+32} H{a}" fill="none" stroke="#577285" stroke-width="2" marker-end="url(#arrow)"/>')
    for n in nodes:
        x, y = coords[n['id']]; color = COLORS.get(n['type'], '#4d7b6c')
        bypass = n.get('bypass')
        dash = ' stroke-dasharray="5 3"' if bypass else ''
        parts.extend([f'<g><title>{escape((n.get("label") or n["id"]))} · {escape(n["type"])}</title>',
                      f'<rect x="{x}" y="{y}" width="140" height="65" rx="8" fill="white" stroke="{color}" stroke-width="2"{dash}/>',
                      f'<rect x="{x}" y="{y}" width="5" height="65" rx="2" fill="{color}"/>',
                      f'<text x="{x+12}" y="{y+25}" font-family="Helvetica,Arial,sans-serif" font-size="13" fill="#20364c">{escape((n.get("label") or n["id"])[:19])}</text>',
                      f'<text x="{x+12}" y="{y+47}" font-family="Helvetica,Arial,sans-serif" font-size="11" fill="#52687b">{escape(n["type"])} · {escape(n.get("active_channel") or "Unknown channel")}{ " · bypassed" if bypass is True else " · enabled" if bypass is False else " · bypass unknown"}</text></g>'])
    parts.extend([f'<text x="32" y="{height-25}" font-family="Helvetica,Arial,sans-serif" font-size="12" fill="#52687b">Dashed block = bypassed · cables come from graph data · positions use one-based rows/columns</text>', '</svg>'])
    return '\n'.join(parts)


def block_svg(name, node, status):
    parameters = dict(node.get('parameters', {}))
    for ch, values in node.get('channels', {}).items():
        values = values.get('parameters', values)
        for key, value in values.items():
            parameters[f'{ch} · {key}'] = value.get('display', value.get('value')) if isinstance(value, dict) else value
    rows = [('Type', node['type']), ('Bypass', 'unknown' if node.get('bypass') is None else 'on' if node['bypass'] else 'off'),
            ('Active channel', node.get('active_channel') or 'unknown'), *parameters.items()]
    height = 155 + 30 * len(rows)
    text = [f'<svg xmlns="http://www.w3.org/2000/svg" width="700" height="{height}" role="img">',
            f'<rect width="700" height="{height}" rx="12" fill="#edf2f6"/>',
            f'<text x="30" y="38" font-family="Helvetica,Arial,sans-serif" font-size="22" fill="#20364c">{escape((node.get("label") or node["id"]))}</text>',
            f'<text x="30" y="65" font-family="Helvetica,Arial,sans-serif" font-size="12" fill="#52687b">{escape(name)} · {escape(status)}</text>']
    for i, (key, value) in enumerate(rows):
        y = 105 + i * 30
        text.append(f'<text x="30" y="{y}" font-family="Helvetica,Arial,sans-serif" font-size="13" fill="#52687b">{escape(str(key))}</text>')
        text.append(f'<text x="330" y="{y}" font-family="Helvetica,Arial,sans-serif" font-size="13" fill="#20364c">{escape(str(value))}</text>')
    text.append('</svg>')
    return '\n'.join(text)


def render_file(source, output, physical=False, block_id=None, scene_number=None):
    output = Path(output).expanduser().resolve()
    drawing = svg(source, physical, block_id, scene_number)
    require(output.suffix.lower() in ('.svg', '.png'), 'INVALID_FORMAT', 'Choose .svg or .png output')
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.suffix.lower() == '.svg':
        output.write_text(drawing, encoding='utf8')
    else:
        try:
            from PIL import Image, ImageDraw, ImageFont
        except ImportError as exc:
            raise AxeError('OPTIONAL_DEPENDENCY', 'CLI PNG export requires config/requirements/visual.txt; the browser can export PNG without it') from exc
        import re
        import xml.etree.ElementTree as ET
        # Rasterize only our own generated SVG vocabulary from identical graph geometry.
        root = ET.fromstring(drawing); scale = 2
        image = Image.new('RGB', (int(root.attrib['width']) * scale, int(root.attrib['height']) * scale), 'white')
        canvas = ImageDraw.Draw(image)
        for element in root:
            tag = element.tag.rsplit('}', 1)[-1]
            items = list(element) if tag == 'g' else [element]
            for item in items:
                tag = item.tag.rsplit('}', 1)[-1]; a = item.attrib
                if tag == 'rect':
                    x, y = float(a.get('x', 0)) * scale, float(a.get('y', 0)) * scale
                    box = (x, y, x + float(a['width']) * scale, y + float(a['height']) * scale)
                    canvas.rounded_rectangle(box, radius=float(a.get('rx', 0)) * scale, fill=a.get('fill'),
                                             outline=a.get('stroke'), width=max(1, int(float(a.get('stroke-width', 1)) * scale)))
                    if 'stroke-dasharray' in a:
                        for start in range(int(x), int(box[2]), 16):
                            canvas.line((start, y, min(start + 7, box[2]), y), fill='white', width=3)
                elif tag == 'path' and a.get('marker-end'):
                    coords = re.findall(r'-?\d+(?:\.\d+)?', a['d'])
                    x, y, middle, target_y, target_x = map(float, coords)
                    points = [(x*scale,y*scale),(middle*scale,y*scale),(middle*scale,target_y*scale),(target_x*scale,target_y*scale)]
                    canvas.line(points, fill=a['stroke'], width=2*scale)
                    canvas.polygon([(target_x*scale,target_y*scale),((target_x-7)*scale,(target_y-4)*scale),((target_x-7)*scale,(target_y+4)*scale)], fill=a['stroke'])
                elif tag == 'text':
                    size = int(float(a.get('font-size', 13)) * scale)
                    font = None
                    for candidate in ('/System/Library/Fonts/Helvetica.ttc', 'DejaVuSans.ttf', 'Arial.ttf'):
                        try:
                            font = ImageFont.truetype(candidate, size); break
                        except OSError:
                            continue
                    font = font or ImageFont.load_default(size=size)
                    canvas.text((float(a['x'])*scale,float(a['y'])*scale), ''.join(item.itertext()),
                                font=font, fill=a.get('fill', '#20364c'), anchor='ls')
        image.resize((image.width//scale,image.height//scale), Image.Resampling.LANCZOS).save(output)
    return {'path': str(output), 'format': output.suffix[1:], 'physical': physical, 'block': block_id}
