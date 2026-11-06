// Read-only agent tools over the local clip library: list, analyse, compare.
// The agent receives numeric reports only, never audio or file paths.
#import <Foundation/Foundation.h>
@class AxeAudioClips;

@interface AxeAudioTools : NSObject
+ (NSArray<NSDictionary *> *)specifications;
/// Runs analysis off the main thread and replies on the main thread.
+ (void)call:(NSString *)name arguments:(NSDictionary *)arguments clips:(AxeAudioClips *)clips
       reply:(void (^)(NSDictionary *result))reply;
@end
