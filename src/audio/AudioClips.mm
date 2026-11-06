#import "AudioClips.h"
#import <AVFoundation/AVFoundation.h>
#include <sys/stat.h>

NSNotificationName const AxeAudioClipsChangedNotification = @"AxeAudioClipsChanged";
static const unsigned long long kMaxFileBytes = 200ULL * 1024 * 1024;
static const NSUInteger kMaxClips = 50;

@interface AxeAudioClips ()
@property(nonatomic, strong) NSURL *directory;
@property(nonatomic, strong) NSMutableArray<NSMutableDictionary *> *index;
@end

@implementation AxeAudioClips
+ (instancetype)shared {
    static AxeAudioClips *clips; static dispatch_once_t once;
    dispatch_once(&once, ^{
        NSURL *support = [NSFileManager.defaultManager URLsForDirectory:NSApplicationSupportDirectory inDomains:NSUserDomainMask].firstObject;
        clips = [[AxeAudioClips alloc] initWithDirectory:[support URLByAppendingPathComponent:@"AxeStudioEmbeddedChat/Audio"]];
    });
    return clips;
}
- (instancetype)initWithDirectory:(NSURL *)directory {
    if ((self = [super init])) {
        _directory = directory;
        [NSFileManager.defaultManager createDirectoryAtURL:directory withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0700} error:nil];
        NSData *data = [NSData dataWithContentsOfURL:[directory URLByAppendingPathComponent:@"clips.json"]];
        id saved = data ? [NSJSONSerialization JSONObjectWithData:data options:NSJSONReadingMutableContainers error:nil] : nil;
        _index = [NSMutableArray array];
        if ([saved isKindOfClass:NSArray.class])
            for (id clip in saved)
                if ([clip isKindOfClass:NSDictionary.class] && [clip[@"id"] isKindOfClass:NSString.class] &&
                    [NSFileManager.defaultManager fileExistsAtPath:[directory URLByAppendingPathComponent:clip[@"file"] ?: @""].path])
                    [_index addObject:[clip mutableCopy]];
    }
    return self;
}
- (void)save {
    NSData *data = [NSJSONSerialization dataWithJSONObject:self.index options:0 error:nil];
    NSURL *file = [self.directory URLByAppendingPathComponent:@"clips.json"];
    if ([data writeToURL:file options:NSDataWritingAtomic error:nil]) chmod(file.fileSystemRepresentation, 0600);
    [NSNotificationCenter.defaultCenter postNotificationName:AxeAudioClipsChangedNotification object:self];
}
static NSDictionary *visible(NSDictionary *clip) {
    NSMutableDictionary *copy = [clip mutableCopy]; [copy removeObjectForKey:@"file"]; return copy;
}
- (NSArray *)clips {
    NSMutableArray *out = [NSMutableArray array];
    for (NSDictionary *clip in self.index.reverseObjectEnumerator) [out addObject:visible(clip)];
    return out;
}
- (NSMutableDictionary *)entry:(NSString *)identifier {
    for (NSMutableDictionary *clip in self.index) if ([clip[@"id"] isEqual:identifier]) return clip;
    return nil;
}
- (NSDictionary *)clipWithID:(NSString *)identifier { NSDictionary *clip = [self entry:identifier]; return clip ? visible(clip) : nil; }
- (NSURL *)fileForClip:(NSString *)identifier {
    NSDictionary *clip = [self entry:identifier];
    return clip ? [self.directory URLByAppendingPathComponent:clip[@"file"]] : nil;
}
- (NSDictionary *)add:(NSURL *)source name:(NSString *)name role:(NSString *)role origin:(NSString *)origin
                 move:(BOOL)move failure:(NSString **)failure {
    NSString *why = nil;
    NSNumber *size = [NSFileManager.defaultManager attributesOfItemAtPath:source.path error:nil][NSFileSize];
    NSError *error = nil;
    AVAudioFile *audio = [[AVAudioFile alloc] initForReading:source error:&error];
    if (!size || size.unsignedLongLongValue == 0) why = @"The file is empty or unreadable";
    else if (size.unsignedLongLongValue > kMaxFileBytes) why = @"Audio files are limited to 200 MB";
    else if (!audio || audio.length == 0) why = @"This file is not a supported audio format";
    else if (self.index.count >= kMaxClips) why = @"The clip library is full (50 clips); delete some clips first";
    if (why) { if (failure) *failure = why; return nil; }
    NSString *identifier = [[NSUUID.UUID.UUIDString substringToIndex:8] lowercaseString];
    NSString *extension = source.pathExtension.length ? source.pathExtension.lowercaseString : @"caf";
    NSString *fileName = [identifier stringByAppendingPathExtension:extension];
    NSURL *target = [self.directory URLByAppendingPathComponent:fileName];
    BOOL ok = move ? [NSFileManager.defaultManager moveItemAtURL:source toURL:target error:&error]
                   : [NSFileManager.defaultManager copyItemAtURL:source toURL:target error:&error];
    if (!ok) { if (failure) *failure = @"The audio could not be stored in the clip library"; return nil; }
    chmod(target.fileSystemRepresentation, 0600);
    NSMutableDictionary *clip = [@{@"id":identifier, @"name":name, @"role":role, @"source":origin,
        @"duration_s":@(round(audio.length / audio.processingFormat.sampleRate * 10) / 10),
        @"created":[NSISO8601DateFormatter stringFromDate:NSDate.date timeZone:NSTimeZone.localTimeZone formatOptions:NSISO8601DateFormatWithInternetDateTime],
        @"file":fileName} mutableCopy];
    [self.index addObject:clip];
    [self save];
    return visible(clip);
}
- (NSDictionary *)importFile:(NSURL *)file failure:(NSString **)failure {
    return [self add:file name:file.lastPathComponent role:@"reference" origin:@"file" move:NO failure:failure];
}
- (NSDictionary *)addRecording:(NSURL *)file device:(NSString *)device failure:(NSString **)failure {
    NSDateFormatter *format = [NSDateFormatter new]; format.dateFormat = @"HH:mm:ss";
    NSString *name = [NSString stringWithFormat:@"My tone %@", [format stringFromDate:NSDate.date]];
    return [self add:file name:name role:@"current" origin:device ?: @"input" move:YES failure:failure];
}
- (void)setRole:(NSString *)role forClip:(NSString *)identifier {
    if (![@[@"reference", @"current"] containsObject:role]) return;
    [self entry:identifier][@"role"] = role; [self save];
}
- (void)removeClip:(NSString *)identifier {
    NSMutableDictionary *clip = [self entry:identifier];
    if (!clip) return;
    [NSFileManager.defaultManager removeItemAtURL:[self.directory URLByAppendingPathComponent:clip[@"file"]] error:nil];
    [self.index removeObject:clip]; [self save];
}
@end
