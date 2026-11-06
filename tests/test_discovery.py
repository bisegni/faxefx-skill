"""Static discovery: instruction-shape decoder, and the installed 1.15.03 build
must yield exactly the hand-verified bridge profile (skipped if absent)."""
import hashlib
import importlib.util
import struct
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('discover', ROOT / 'scripts/discover_axe_edit.py')
discover = importlib.util.module_from_spec(spec)
spec.loader.exec_module(discover)
APP = Path('/Applications/Axe-Edit III.app')
KNOWN_SHA = '647ba24fb04c2cfcbcb972b32f338b0422eb87c90ca7898cf26f39e9c5bd8435'


def ldr(kind, rt, rn, offset):
    base = {'ldrh': 0x79400000, 'ldrw': 0xB9400000, 'ldrs': 0xBD400000}[kind]
    scale = 2 if kind == 'ldrh' else 4
    return base | ((offset // scale) << 10) | (rn << 5) | rt


class ShapeTest(unittest.TestCase):
    MOV_X21_X1 = 0xAA0103F5
    BL = 0x94000010
    NOP = 0xD503201F

    def record_copy(self, reg):
        return [ldr('ldrh', 8, reg, 0), self.NOP, ldr('ldrh', 9, reg, 2), ldr('ldrw', 10, reg, 4), ldr('ldrs', 0, reg, 8)]

    def test_observed_sequence_through_moved_register(self):
        self.assertTrue(discover.command_shape([self.MOV_X21_X1] + [self.NOP] * 5 + self.record_copy(21) + [self.NOP, self.BL]))

    def test_rejects_other_register_missing_field_or_no_call(self):
        self.assertFalse(discover.command_shape(self.record_copy(21) + [self.BL]))          # x21 never held x1
        self.assertFalse(discover.command_shape(self.record_copy(1)[:-1] + [self.BL]))     # no float at +8
        self.assertFalse(discover.command_shape(self.record_copy(1) + [self.NOP] * 30))    # no call


@unittest.skipUnless((APP / 'Contents/MacOS/Axe-Edit III').exists(), 'Axe-Edit III not installed')
class InstalledBuildTest(unittest.TestCase):
    def test_1_15_03_matches_verified_profile(self):
        binary = APP / 'Contents/MacOS/Axe-Edit III'
        if hashlib.sha256(binary.read_bytes()).hexdigest() != KNOWN_SHA:
            self.skipTest('installed Axe-Edit is not 1.15.03')
        profile = discover.discover(APP)
        self.assertEqual(profile['command_method'], 0x1000C8D9C)
        self.assertEqual(profile['comm_typeinfo_slot'], 0x101140100)
        self.assertEqual(profile['comm_slot'], 0x1011401A8)
        self.assertEqual(profile['content_vtable'], 0x1011682D8)
        self.assertEqual((profile['content_comm_vtable'], profile['content_comm_slot'], profile['content_comm_offset']),
                         (0x1011685C0, 0x101168660, 0x118))
        self.assertEqual(profile['text'], {'address': 0x10000210C, 'size': 0x610EA8,
                         'sha256': 'e7aa716d8c58501a43408ab2643d1f51a3b94fe8073871a4ea9d9e66f9037e45'})
        self.assertEqual(profile['command_method_users'], 25)


if __name__ == '__main__':
    unittest.main()
