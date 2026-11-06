#!/usr/bin/env python3
"""Statically locate Axe-Edit III's parameter-command path in an arm64 build.

Produces a "bridge profile" for src/editor/NativeBridge.mm:
  - A3ComponentComm: its RTTI, primary vtable, and the shared command method
    at vtable slot +0xa0 (the method every knob edit goes through);
  - A3EffectEditorContent: its primary vtable and the A3ComponentComm base
    (vtable address and offset inside the object);
  - the __text section identity (address, size, SHA-256) the library checks
    at runtime so the profile can only be used with this exact binary.

The method itself is accepted only if its code has the observed shape: it
copies the 12-byte record {u16 effect, u16 parameter, u32 op, f32 raw} from
its record argument (x1) and then calls the serializer.

Read-only: uses Python's stdlib plus `xcrun dyld_info` for chained fixups.
Usage: scripts/discover_axe_edit.py [APP_OR_EXECUTABLE] [--json]
"""
import hashlib
import json
import struct
import subprocess
import sys
from pathlib import Path

CPU_ARM64 = 0x0100000C
FAT_MAGIC, FAT_MAGIC_64, MH_MAGIC_64 = 0xCAFEBABE, 0xCAFEBABF, 0xFEEDFACF
LC_SEGMENT_64 = 0x19
COMM_CLASS, CONTENT_CLASS = b'15A3ComponentComm', b'21A3EffectEditorContent'
COMMAND_SLOT = 0xA0


class DiscoveryError(Exception):
    pass


def executable_for(path):
    path = Path(path)
    if path.suffix == '.app':
        info = path / 'Contents/Info.plist'
        import plistlib
        name = plistlib.loads(info.read_bytes()).get('CFBundleExecutable', 'Axe-Edit III')
        original = path / 'Contents/MacOS' / f'{name} Original'
        return original if original.exists() else path / 'Contents/MacOS' / name
    return path


def arm64_slice(data):
    magic = struct.unpack_from('>I', data, 0)[0]
    if magic in (FAT_MAGIC, FAT_MAGIC_64):
        count = struct.unpack_from('>I', data, 4)[0]
        for i in range(count):
            if magic == FAT_MAGIC:
                cpu, _sub, offset, size, _align = struct.unpack_from('>iiIII', data, 8 + 20 * i)
            else:
                cpu, _sub, offset, size, _align, _res = struct.unpack_from('>iiQQII', data, 8 + 32 * i)
            if cpu == CPU_ARM64:
                return offset, size
        raise DiscoveryError('no arm64 slice')
    if struct.unpack_from('<I', data, 0)[0] == MH_MAGIC_64:
        return 0, len(data)
    raise DiscoveryError('not a Mach-O executable')


class Image:
    """Minimal arm64 Mach-O view: segments, sections and VM reads."""

    def __init__(self, path):
        self.path = Path(path)
        data = self.path.read_bytes()
        self.slice_offset, size = arm64_slice(data)
        self.data = data[self.slice_offset:self.slice_offset + size]
        magic, cpu, _sub, _ftype, ncmds, _sizeofcmds, _flags = struct.unpack_from('<IiiIIII', self.data, 0)
        if magic != MH_MAGIC_64 or cpu != CPU_ARM64:
            raise DiscoveryError('arm64 slice is not a 64-bit Mach-O')
        self.segments, self.sections = [], {}
        pos = 32
        for _ in range(ncmds):
            cmd, cmdsize = struct.unpack_from('<II', self.data, pos)
            if cmd == LC_SEGMENT_64:
                segname = self.data[pos + 8:pos + 24].rstrip(b'\0').decode()
                vmaddr, vmsize, fileoff, filesize = struct.unpack_from('<QQQQ', self.data, pos + 24)
                nsects = struct.unpack_from('<I', self.data, pos + 64)[0]
                self.segments.append((segname, vmaddr, vmsize, fileoff, filesize))
                for s in range(nsects):
                    base = pos + 72 + 80 * s
                    sectname = self.data[base:base + 16].rstrip(b'\0').decode()
                    seg = self.data[base + 16:base + 32].rstrip(b'\0').decode()
                    addr, ssize, offset = struct.unpack_from('<QQI', self.data, base + 32)
                    self.sections[(seg, sectname)] = (addr, ssize, offset)
            pos += cmdsize
        self.base = next(v for n, v, *_ in self.segments if n == '__TEXT')

    def offset(self, address):
        for _name, vmaddr, vmsize, fileoff, filesize in self.segments:
            if vmaddr <= address < vmaddr + min(vmsize, filesize):
                return fileoff + address - vmaddr
        raise DiscoveryError(f'address {address:#x} is not file-backed')

    def u64(self, address):
        return struct.unpack_from('<Q', self.data, self.offset(address))[0]

    def i64(self, address):
        return struct.unpack_from('<q', self.data, self.offset(address))[0]

    def words(self, address, count):
        return struct.unpack_from(f'<{count}I', self.data, self.offset(address))

    def text(self):
        return self.sections[('__TEXT', '__text')]

    def in_text(self, address):
        addr, size, _ = self.text()
        return addr <= address < addr + size

    def cstring_addresses(self, needle):
        # RTTI names live in __cstring or __const depending on the toolchain.
        found = []
        for key in (('__TEXT', '__cstring'), ('__TEXT', '__const')):
            if key not in self.sections:
                continue
            addr, size, offset = self.sections[key]
            blob, start = self.data[offset:offset + size], 0
            while (i := blob.find(needle + b'\0', start)) >= 0:
                if i == 0 or blob[i - 1] == 0:
                    found.append(addr + i)
                start = i + 1
        return found


def fixups(path):
    """Rebase locations -> unslid targets, from dyld's own chained-fixup decoder."""
    out = subprocess.run(['xcrun', 'dyld_info', '-fixups', '-arch', 'arm64', str(path)],
                         capture_output=True, text=True)
    if out.returncode:
        raise DiscoveryError('xcrun dyld_info failed: ' + out.stderr.strip())
    table = {}
    for line in out.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 5 and parts[3] == 'rebase':
            table[int(parts[2], 16)] = int(parts[4], 16)
    if not table:
        raise DiscoveryError('no rebase fixups found')
    return table


def vtable_groups(image, table, class_name):
    """All vtable groups of a class: (address point, offset_to_top, typeinfo slot)."""
    names = image.cstring_addresses(class_name)
    reverse = {}
    for loc, target in table.items():
        reverse.setdefault(target, []).append(loc)
    groups = set()
    for name in names:
        for name_slot in reverse.get(name, []):
            typeinfo = name_slot - 8          # type_info: {vtable ptr, name ptr, ...}
            for slot in reverse.get(typeinfo, []):
                point = slot + 8
                # A vtable group is {offset_to_top (plain int), typeinfo ptr, functions...}.
                if slot - 8 in table or point not in table or not image.in_text(table[point]):
                    continue
                groups.add((point, image.i64(slot - 8), slot))
    return sorted(groups)


def decode(word):
    """The few A64 encodings the shape check needs."""
    if word & 0xFFE0FFE0 == 0xAA0003E0:                       # mov Xd, Xm (orr xd, xzr, xm)
        return ('mov', word & 31, (word >> 16) & 31)
    if word & 0xFFC00000 == 0x79400000:                       # ldrh Wt, [Xn, #imm*2]
        return ('ldrh', word & 31, (word >> 5) & 31, ((word >> 10) & 0xFFF) * 2)
    if word & 0xFFC00000 == 0xB9400000:                       # ldr Wt, [Xn, #imm*4]
        return ('ldrw', word & 31, (word >> 5) & 31, ((word >> 10) & 0xFFF) * 4)
    if word & 0xFFC00000 == 0xBD400000:                       # ldr St, [Xn, #imm*4]
        return ('ldrs', word & 31, (word >> 5) & 31, ((word >> 10) & 0xFFF) * 4)
    if word & 0xFC000000 == 0x94000000:                       # bl
        return ('bl',)
    return None


def command_shape(words):
    """True if the code copies {u16@0, u16@2, u32@4, f32@8} from the record
    argument (x1, possibly moved to a callee-saved register) and soon calls."""
    record_regs = {1}
    decoded = [decode(w) for w in words]
    for i, op in enumerate(decoded):
        if op and op[0] == 'mov' and op[2] in record_regs:
            record_regs.add(op[1])
        for reg in record_regs:
            window = [d for d in decoded[i:i + 8] if d and d[0] in ('ldrh', 'ldrw', 'ldrs') and d[2] == reg]
            seen = {(d[0], d[3]) for d in window}
            if {('ldrh', 0), ('ldrh', 2), ('ldrw', 4), ('ldrs', 8)} <= seen:
                if any(d and d[0] == 'bl' for d in decoded[i:i + 20]):
                    return True
    return False


def discover(path):
    executable = executable_for(path)
    image = Image(executable)
    table = fixups(executable)
    text_addr, text_size, text_offset = image.text()
    text_sha = hashlib.sha256(image.data[text_offset:text_offset + text_size]).hexdigest()

    comm = [g for g in vtable_groups(image, table, COMM_CLASS) if g[1] == 0]
    if len(comm) != 1:
        raise DiscoveryError(f'expected one A3ComponentComm primary vtable, found {len(comm)}')
    comm_point, _, comm_typeinfo_slot = comm[0]
    comm_slot = comm_point + COMMAND_SLOT
    method = table.get(comm_slot)
    if method is None or not image.in_text(method):
        raise DiscoveryError('A3ComponentComm vtable slot +0xa0 is not a code pointer')
    if not command_shape(image.words(method, 0x600 // 4)):
        raise DiscoveryError(f'method {method:#x} does not have the parameter-command shape')

    content = vtable_groups(image, table, CONTENT_CLASS)
    primary = [g for g in content if g[1] == 0]
    comm_bases = [g for g in content if table.get(g[0] + COMMAND_SLOT) == method]
    if len(primary) != 1 or len(comm_bases) != 1:
        raise DiscoveryError(f'A3EffectEditorContent layout not recognised (primary {len(primary)}, command bases {len(comm_bases)})')
    content_comm_point, content_comm_ott, _ = comm_bases[0]
    users = sorted(loc for loc, target in table.items() if target == method)

    return {
        'schema': 1,
        'executable': str(executable),
        'executable_sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
        'arch': 'arm64',
        'text': {'address': text_addr, 'size': text_size, 'sha256': text_sha},
        'command_method': method,
        'comm_typeinfo_slot': comm_typeinfo_slot,
        'comm_vtable': comm_point,
        'comm_slot': comm_slot,
        'content_vtable': primary[0][0],
        'content_comm_vtable': content_comm_point,
        'content_comm_slot': content_comm_point + COMMAND_SLOT,
        'content_comm_offset': -content_comm_ott,
        'command_method_users': len(users),
    }


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    target = args[0] if args else '/Applications/Axe-Edit III.app'
    try:
        profile = discover(target)
    except (DiscoveryError, OSError, KeyError) as error:
        sys.exit(f'discovery failed: {error}')
    if '--json' in sys.argv:
        print(json.dumps(profile, indent=2))
        return
    for key in ('command_method', 'comm_vtable', 'comm_slot', 'content_vtable', 'content_comm_vtable', 'content_comm_slot'):
        print(f'{key:22} {profile[key]:#x}')
    print(f'{"content_comm_offset":22} {profile["content_comm_offset"]:#x}')
    print(f'{"text sha256":22} {profile["text"]["sha256"]}')
    print(f'{"vtable users":22} {profile["command_method_users"]}')


if __name__ == '__main__':
    main()
