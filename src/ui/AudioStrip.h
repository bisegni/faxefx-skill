// Collapsible Audio section of the Tone Assistant panel: choose an input and
// record the current tone, import or drop reference files, play, mark the
// role of each clip and delete clips.
#import <Cocoa/Cocoa.h>

@interface AxeAudioStrip : NSView
@property(nonatomic) BOOL expanded;
/// Height this strip wants at the current width (collapsed or expanded).
- (CGFloat)preferredHeight;
/// Called when the preferred height changes (expand/collapse, clips added).
@property(nonatomic, copy) void (^heightChanged)(void);
@end
