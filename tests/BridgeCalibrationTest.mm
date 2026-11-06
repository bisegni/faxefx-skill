// Runtime layout calibration against real C++ objects whose RTTI names match
// Axe-Edit's classes, laid out like the verified 1.15.03 build (with decoys).
#import "NativeBridge.h"
#include <assert.h>
#include <malloc/malloc.h>
#include <cstring>
#include <vector>

struct A3EffectEditorContent { virtual ~A3EffectEditorContent() {} uint8_t body[0x200]; };
struct A3RotaryKnob { virtual ~A3RotaryKnob() {} uint8_t body[0x400]; };
struct UndoManager { virtual ~UndoManager() {} uint8_t body[0x40]; };
struct FakeComm { virtual ~FakeComm() {} uint8_t before[0x2f8]; UndoManager undo; };   // undo at +0x300
struct TwoUndo { virtual ~TwoUndo() {} uint8_t a[0x1f8]; UndoManager first; uint8_t b[0x80]; UndoManager second; };

template <typename T> static void put(void *object, size_t offset, T value) { std::memcpy((uint8_t *)object + offset, &value, sizeof value); }
static uintptr_t vtableOf(void *object) { return *(uintptr_t *)object; }

int main(void) {
    @autoreleasepool {
        // Firmware definitions for type 10 (Amplifier): raw min, max, scale, step, list?, flags.
        NSDictionary *amp = @{@1:@[@0, @1, @10, @0.001, @NO, @50], @2:@[@0, @1, @10, @0.001, @NO, @50],
            @3:@[@0, @1, @10, @0.001, @NO, @50], @21:@[@-80, @20, @1, @0.1, @NO, @0], @30:@[@0, @3, @1, @1, @YES, @0]};
        uint32_t (^typeFor)(uint16_t) = ^uint32_t(uint16_t e) { return e >= 58 && e < 62 ? 10 : 0; };
        NSArray *(^recordFor)(uint32_t, uint16_t) = ^NSArray *(uint32_t t, uint16_t p) { return t == 10 ? amp[@(p)] : nil; };

        A3EffectEditorContent *content = new A3EffectEditorContent(), *other = new A3EffectEditorContent();
        UndoManager *decoyObject = new UndoManager();
        std::vector<A3RotaryKnob *> knobs;
        const uint16_t params[] = {1, 2, 3, 21, 30};
        for (uint16_t p : params) {
            A3RotaryKnob *knob = new A3RotaryKnob();
            std::memset(knob->body, 0, sizeof knob->body);
            put<void *>(knob, 0x18, decoyObject);            // a pointer that is not the editor
            put<void *>(knob, 0x30, content);                // parent editor
            put<uint16_t>(knob, 0x210, 58); put<uint16_t>(knob, 0x212, p);
            put<uint16_t>(knob, 0x218, 58); put<uint16_t>(knob, 0x21a, p);   // duplicate copy, as observed
            NSArray *r = amp[@(p)];
            put<float>(knob, 0x23c, [r[4] boolValue] ? 0.f : [r[0] floatValue] * [r[2] floatValue]);
            put<float>(knob, 0x240, [r[4] boolValue] ? 7.f : [r[1] floatValue] * [r[2] floatValue]);
            put<float>(knob, 0x260, 1.f); put<float>(knob, 0x264, 0.6f);     // unrelated floats
            knobs.push_back(knob);
        }
        NSMutableArray *addresses = [NSMutableArray array];
        for (A3RotaryKnob *knob : knobs) [addresses addObject:@((uint64_t)(uintptr_t)knob)];
        NSString *why = nil;
        NSDictionary *layout = AxeCalibrateKnobLayout(addresses, vtableOf(content), typeFor, recordFor, &why);
        assert(layout);
        assert([layout[@"knob_parent"] isEqual:@0x30] && [layout[@"knob_effect"] isEqual:@0x210] && [layout[@"knob_parameter"] isEqual:@0x212]);
        assert([layout[@"knob_min"] isEqual:@0x23c] && [layout[@"knob_max"] isEqual:@0x240]);
        puts("PASS: calibration recovers parent +0x30, IDs +0x210/+0x212 and range +0x23c/+0x240 (list knob ignored, decoys skipped)");

        assert(!AxeCalibrateKnobLayout([addresses subarrayWithRange:NSMakeRange(0, 2)], vtableOf(content), typeFor, recordFor, &why));
        put<void *>(knobs[1], 0x30, other);
        assert(!AxeCalibrateKnobLayout(addresses, vtableOf(content), typeFor, recordFor, &why) && [why containsString:@"parent"]);
        put<void *>(knobs[1], 0x30, content);
        NSArray *(^wrongScale)(uint32_t, uint16_t) = ^NSArray *(uint32_t t, uint16_t p) {
            NSMutableArray *r = [amp[@(p)] mutableCopy]; r[2] = @([r[2] doubleValue] * 2); return t == 10 ? r : nil; };
        assert(!AxeCalibrateKnobLayout(addresses, vtableOf(content), typeFor, wrongScale, &why) && [why containsString:@"range"]);
        assert(!AxeCalibrateKnobLayout(addresses, vtableOf(content), ^uint32_t(uint16_t e) { return 0; }, recordFor, &why) && [why containsString:@"IDs"]);
        puts("PASS: calibration refuses too few knobs, mixed parents, inconsistent ranges and unknown blocks");

        FakeComm *comm = new FakeComm();
        assert(AxeFindUndoOffset((uintptr_t)comm, malloc_size(comm)) == 0x300);
        TwoUndo *ambiguous = new TwoUndo();
        assert(AxeFindUndoOffset((uintptr_t)ambiguous, malloc_size(ambiguous)) == -1);
        assert(AxeFindUndoOffset((uintptr_t)content, malloc_size(content)) == -1);   // neighbours are out of bounds
        puts("PASS: Undo context found at +0x300 by RTTI within the allocation; ambiguous or absent is refused");
    }
    return 0;
}
