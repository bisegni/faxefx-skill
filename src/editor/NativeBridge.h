// Bridge into Axe-Edit III's own parameter command path (A3ComponentComm
// vtable slot +0xa0). Addresses come from a "bridge profile":
//   - built in for the hand-verified Axe-Edit III 1.15.03 arm64, or
//   - discovered by scripts/discover_axe_edit.py when the copy was patched
//     (Contents/Resources/AxeBridgeProfile.json).
// Memory-layout offsets (knob IDs/range, parent, Undo context) are known for
// 1.15.03 and calibrated at runtime for other builds. Writes on a discovered,
// not yet confirmed build stay off until the user enables them once.
// Evidence: docs/AXE_EDIT_PARAMETER_UPDATE_HANDOFF.md.
#import <Foundation/Foundation.h>
#import "EditorTools.h"

@interface AxeNativeBridge : NSObject <AxeParameterWriter>
+ (instancetype)shared;
/// YES when the executable's __text matches the active profile's identity
/// and the profile's RTTI and vtable slots check out in memory.
@property(nonatomic, readonly) BOOL available;
@property(nonatomic, readonly, copy) NSString *unavailableReason;
/// "verified-1.15.03", "discovered" or nil (unavailable).
@property(nonatomic, readonly, copy) NSString *profileKind;
/// YES for a discovered build whose writes the user has not enabled yet.
@property(nonatomic, readonly) BOOL needsUserEnable;
/// Enable knob writes for this discovered build (remembered per code hash).
- (void)enableWritesForThisBuild;
/// YES once the runtime layout is known (and, on the verified build, cross-checked).
@property(nonatomic, readonly) BOOL calibrationSettled;
/// Calibrate runtime offsets from the visible knobs (no-op once known).
- (void)calibrateWithAccessibilityNodes:(NSArray *)nodes;
/// Resolve an accessibility slider to its A3RotaryKnob's parameter identity
/// and conversion. Returns nil with a reason unless every RTTI/vtable check,
/// the firmware cache record and the knob's own display range agree.
/// Keys: effect, parameter, effect_type, display_min, display_max, scale,
/// step, raw_min, raw_max, cache.
- (NSDictionary *)parameterForAccessibilityNode:(id)node failure:(NSString **)failure;
/// Send a display value through the editor's own Undo-aware command
/// (op 0x09, flag 0), exactly like a typed knob entry. Main thread only.
- (BOOL)setDisplayValue:(double)value forAccessibilityNode:(id)node
                failure:(NSString **)failure;
@end

/// Pure conversion used by the bridge (exposed for unit tests).
/// Returns NO if the value is outside the record or the knob range disagrees.
BOOL AxeNativeDisplayToRaw(double display, double rawMin, double rawMax, double scale,
                           double knobMin, double knobMax, float *raw, NSString **failure);

/// Runtime layout calibration over live (or test) A3RotaryKnob objects that
/// share one A3EffectEditorContent parent. Finds the parent-pointer offset,
/// the u16 effect/parameter ID offsets and the float display-range offsets
/// by self-consistency with the firmware definitions. Returns keys
/// knob_parent, knob_effect, knob_parameter, knob_min, knob_max, or nil.
NSDictionary *AxeCalibrateKnobLayout(NSArray<NSNumber *> *knobs, uintptr_t contentVtable,
    uint32_t (^typeForInstance)(uint16_t effect),
    NSArray *(^recordFor)(uint32_t type, uint16_t parameter), NSString **failure);
/// Offset of the single embedded object whose RTTI contains "UndoManager" in
/// [0x100, limit) of base, or -1. limit keeps the scan inside the allocation.
long AxeFindUndoOffset(uintptr_t base, size_t limit);
