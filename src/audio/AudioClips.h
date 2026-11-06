// Local clip library for tone matching: imported reference files and
// recordings of the current tone. Audio stays on this Mac.
#import <Foundation/Foundation.h>

extern NSNotificationName const AxeAudioClipsChangedNotification;

@interface AxeAudioClips : NSObject
+ (instancetype)shared;   // ~/Library/Application Support/AxeStudioEmbeddedChat/Audio
- (instancetype)initWithDirectory:(NSURL *)directory;
/// Clips, newest first: id, name, role ("reference" | "current"), source, duration_s, created.
- (NSArray<NSDictionary *> *)clips;
- (NSDictionary *)clipWithID:(NSString *)identifier;
/// Copy an audio file into the library as a reference clip.
- (NSDictionary *)importFile:(NSURL *)file failure:(NSString **)failure;
/// Move a finished recording into the library as a "current" clip.
- (NSDictionary *)addRecording:(NSURL *)file device:(NSString *)device failure:(NSString **)failure;
- (void)setRole:(NSString *)role forClip:(NSString *)identifier;
- (void)removeClip:(NSString *)identifier;
/// File for playback/analysis (never exposed to the agent).
- (NSURL *)fileForClip:(NSString *)identifier;
@end
