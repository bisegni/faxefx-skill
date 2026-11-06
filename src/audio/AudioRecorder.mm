#import "AudioRecorder.h"
#import "AudioClips.h"
#import <AVFoundation/AVFoundation.h>

static const NSTimeInterval kMaxSeconds = 120;

@interface AxeAudioRecorder () <AVCaptureFileOutputRecordingDelegate>
@property(nonatomic) BOOL recording;
@property(nonatomic) NSTimeInterval elapsed;
@property(nonatomic) float level;
@property(nonatomic, strong) AVCaptureSession *session;
@property(nonatomic, strong) AVCaptureAudioFileOutput *output;
@property(nonatomic, copy) NSString *deviceName;
@property(nonatomic, strong) NSTimer *meter;
@property(nonatomic, strong) NSDate *started;
@property(nonatomic, copy) void (^finished)(NSDictionary *, NSString *);
@end

@implementation AxeAudioRecorder
+ (NSArray<AVCaptureDevice *> *)devices {
    NSArray *types;
    if (@available(macOS 14.0, *)) types = @[AVCaptureDeviceTypeMicrophone, AVCaptureDeviceTypeExternal];
    else {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        types = @[AVCaptureDeviceTypeBuiltInMicrophone, AVCaptureDeviceTypeExternalUnknown];
#pragma clang diagnostic pop
    }
    return [AVCaptureDeviceDiscoverySession discoverySessionWithDeviceTypes:types mediaType:AVMediaTypeAudio
        position:AVCaptureDevicePositionUnspecified].devices;
}
+ (NSArray *)inputDevices {
    NSMutableArray *out = [NSMutableArray array];
    for (AVCaptureDevice *device in [self devices]) [out addObject:@{@"id":device.uniqueID, @"name":device.localizedName}];
    return out;
}
+ (void)requestAccess:(void (^)(BOOL))reply {
    AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
    if (status == AVAuthorizationStatusAuthorized) { reply(YES); return; }
    if (status != AVAuthorizationStatusNotDetermined) { reply(NO); return; }
    [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) {
        dispatch_async(dispatch_get_main_queue(), ^{ reply(granted); });
    }];
}
- (void)notify { if (self.changed) self.changed(); }
- (BOOL)startWithDevice:(NSString *)deviceID failure:(NSString **)failure {
    if (self.recording) return YES;
    AVCaptureDevice *device = nil;
    for (AVCaptureDevice *candidate in [AxeAudioRecorder devices]) if ([candidate.uniqueID isEqual:deviceID]) device = candidate;
    if (!device) device = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
    NSError *error = nil;
    AVCaptureDeviceInput *input = device ? [AVCaptureDeviceInput deviceInputWithDevice:device error:&error] : nil;
    if (!input) { if (failure) *failure = @"The audio input could not be opened (check microphone permission)"; return NO; }
    AVCaptureSession *session = [AVCaptureSession new];
    AVCaptureAudioFileOutput *output = [AVCaptureAudioFileOutput new];
    // 48 kHz float, device channel layout kept (silent channels are ignored by the analysis).
    output.audioSettings = @{AVFormatIDKey:@(kAudioFormatLinearPCM), AVSampleRateKey:@48000,
        AVLinearPCMBitDepthKey:@32, AVLinearPCMIsFloatKey:@YES, AVLinearPCMIsBigEndianKey:@NO, AVLinearPCMIsNonInterleaved:@NO};
    output.maxRecordedDuration = CMTimeMakeWithSeconds(kMaxSeconds, 1000);
    if (![session canAddInput:input] || ![session canAddOutput:output]) { if (failure) *failure = @"This input cannot be recorded"; return NO; }
    [session addInput:input]; [session addOutput:output];
    [session startRunning];
    NSURL *file = [NSURL fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:
        [NSString stringWithFormat:@"axe-take-%@.caf", NSUUID.UUID.UUIDString]]];
    [output startRecordingToOutputFileURL:file outputFileType:AVFileTypeCoreAudioFormat recordingDelegate:self];
    self.session = session; self.output = output; self.deviceName = device.localizedName;
    self.recording = YES; self.started = NSDate.date; self.elapsed = 0; self.level = -60;
    __weak AxeAudioRecorder *weakSelf = self;
    self.meter = [NSTimer scheduledTimerWithTimeInterval:0.05 repeats:YES block:^(NSTimer *timer) { [weakSelf tick]; }];
    [self notify];
    return YES;
}
- (void)tick {
    float peak = -60;
    for (AVCaptureConnection *connection in self.output.connections)
        for (AVCaptureAudioChannel *channel in connection.audioChannels) peak = MAX(peak, channel.averagePowerLevel);
    self.level = MAX(-60, MIN(0, peak));
    self.elapsed = -self.started.timeIntervalSinceNow;
    [self notify];
}
- (void)stop:(void (^)(NSDictionary *, NSString *))reply {
    if (!self.recording) { reply(nil, @"Not recording"); return; }
    self.finished = reply;
    [self.output stopRecording];
}
- (void)captureOutput:(AVCaptureFileOutput *)output didFinishRecordingToOutputFileAtURL:(NSURL *)file
      fromConnections:(NSArray *)connections error:(NSError *)error {
    dispatch_async(dispatch_get_main_queue(), ^{
        [self.meter invalidate]; self.meter = nil;
        [self.session stopRunning]; self.session = nil; self.output = nil;
        self.recording = NO; self.level = -60;
        // Reaching the 120 s limit reports an error but still produces a valid file.
        BOOL limit = [error.userInfo[AVErrorRecordingSuccessfullyFinishedKey] boolValue];
        NSString *failure = nil; NSDictionary *clip = nil;
        if (error && !limit) failure = @"The recording failed";
        else clip = [[AxeAudioClips shared] addRecording:file device:self.deviceName failure:&failure];
        if (!clip) [NSFileManager.defaultManager removeItemAtURL:file error:nil];
        void (^reply)(NSDictionary *, NSString *) = self.finished; self.finished = nil;
        [self notify];
        if (reply) reply(clip, failure);
    });
}
@end
