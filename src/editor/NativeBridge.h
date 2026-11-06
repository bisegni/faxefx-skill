// Version-pinned bridge into Axe-Edit III 1.15.03's own parameter command
// path (A3ComponentComm vtable slot +0xa0). Fails closed on any other build.
// See docs/AXE_EDIT_PARAMETER_UPDATE_HANDOFF.md for the evidence.
#import <Foundation/Foundation.h>
#import "EditorTools.h"

@interface AxeNativeBridge : NSObject <AxeParameterWriter>
+ (instancetype)shared;
/// YES only when the loaded executable's __text hash matches 1.15.03 arm64,
/// the A3ComponentComm RTTI is present and its +0xa0 slot targets the method.
@property(nonatomic, readonly) BOOL available;
@property(nonatomic, readonly, copy) NSString *unavailableReason;
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
