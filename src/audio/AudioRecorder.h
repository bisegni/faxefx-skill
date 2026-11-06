// Records from a chosen Mac audio input (e.g. the Axe-Fx III USB interface)
// into the clip library, with a level meter. Main thread only.
#import <Foundation/Foundation.h>

@interface AxeAudioRecorder : NSObject
/// Input devices: id, name. The Axe-Fx USB interface appears by name.
+ (NSArray<NSDictionary *> *)inputDevices;
@property(nonatomic, readonly) BOOL recording;
@property(nonatomic, readonly) NSTimeInterval elapsed;
/// Average input level in dBFS (about -60..0), updated while recording.
@property(nonatomic, readonly) float level;
@property(nonatomic, copy) void (^changed)(void);
/// Ask for microphone access if needed; replies on the main thread.
+ (void)requestAccess:(void (^)(BOOL granted))reply;
- (BOOL)startWithDevice:(NSString *)deviceID failure:(NSString **)failure;
/// Stop and add the take to the clip library; the reply gets the new clip or an error.
- (void)stop:(void (^)(NSDictionary *clip, NSString *failure))reply;
@end
