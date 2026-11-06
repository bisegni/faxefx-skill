// Agent audio tools: schemas, clip validation, comparison, and no path leaks.
#import "EditorTools.h"
#import "AudioClips.h"
#import "AudioTools.h"
#import <AVFoundation/AVFoundation.h>
#include <assert.h>
#include <cmath>

static NSURL *writeTone(NSString *path, double hz, double cutoff) {
    NSURL *url = [NSURL fileURLWithPath:path];
    NSDictionary *settings = @{AVFormatIDKey:@(kAudioFormatLinearPCM), AVSampleRateKey:@48000, AVNumberOfChannelsKey:@1,
        AVLinearPCMBitDepthKey:@32, AVLinearPCMIsFloatKey:@YES, AVLinearPCMIsNonInterleaved:@NO};
    AVAudioFile *file = [[AVAudioFile alloc] initForWriting:url settings:settings commonFormat:AVAudioPCMFormatFloat32 interleaved:NO error:nil];
    AVAudioPCMBuffer *buffer = [[AVAudioPCMBuffer alloc] initWithPCMFormat:file.processingFormat frameCapacity:48000 * 2];
    buffer.frameLength = 48000 * 2;
    // A harmonic-rich saw-like tone; optional one-pole low-pass darkens it.
    double y = 0, a = cutoff > 0 ? std::exp(-2 * M_PI * cutoff / 48000) : 0;
    for (int i = 0; i < 48000 * 2; ++i) {
        double phase = std::fmod(i * hz / 48000, 1.0), v = 0.4 * (2 * phase - 1);
        y = (1 - a) * v + a * y; buffer.floatChannelData[0][i] = (float)(cutoff > 0 ? y : v);
    }
    [file writeFromBuffer:buffer error:nil];
    return url;
}
static NSDictionary *call(AxeEditorTools *tools, NSString *name, NSDictionary *args) {
    __block NSDictionary *result = nil;
    [tools call:name arguments:args authority:^BOOL{ return YES; } reply:^(NSDictionary *value) { result = value; }];
    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:20];
    while (!result && deadline.timeIntervalSinceNow > 0) [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.02]];
    assert(result); return result;
}

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSString *root = [NSTemporaryDirectory() stringByAppendingPathComponent:[@"axe-audio-tools-" stringByAppendingString:NSUUID.UUID.UUIDString]];
        AxeAudioClips *clips = [[AxeAudioClips alloc] initWithDirectory:[NSURL fileURLWithPath:[root stringByAppendingPathComponent:@"Audio"]]];
        AxeEditorTools *tools = [AxeEditorTools shared]; tools.audioClips = clips;
        NSSet *names = [NSSet setWithArray:[[AxeEditorTools specifications] valueForKey:@"name"]];
        assert([names containsObject:@"axefx_audio_list"] && [names containsObject:@"axefx_audio_analyze"] && [names containsObject:@"axefx_audio_compare"]);
        for (NSDictionary *spec in [AxeAudioTools specifications])
            assert([NSJSONSerialization dataWithJSONObject:spec options:0 error:nil] && [spec[@"inputSchema"][@"additionalProperties"] isEqual:@NO]);
        NSDictionary *empty = call(tools, @"axefx_audio_list", @{});
        assert([empty[@"ok"] boolValue] && [empty[@"clips"] count] == 0 && [empty[@"hint"] containsString:@"import"]);
        puts("PASS: audio tool schemas registered with the editor tools; empty library hint");

        NSString *why = nil;
        NSDictionary *reference = [clips importFile:writeTone([root stringByAppendingPathComponent:@"ref.caf"], 110, 0) failure:&why];
        assert(reference && [reference[@"role"] isEqual:@"reference"] && !reference[@"file"]);
        NSURL *take = writeTone([root stringByAppendingPathComponent:@"take.caf"], 110, 900);
        NSDictionary *current = [clips addRecording:take device:@"Axe-Fx III" failure:&why];
        assert(current && [current[@"role"] isEqual:@"current"] && ![NSFileManager.defaultManager fileExistsAtPath:take.path]);
        assert(![clips importFile:[NSURL fileURLWithPath:@"/etc/hosts"] failure:&why] && why.length);
        NSDictionary *list = call(tools, @"axefx_audio_list", @{});
        assert([list[@"clips"] count] == 2 && [list[@"clips"][0][@"id"] isEqual:current[@"id"]]);
        puts("PASS: import as reference, recording moved in as current, non-audio rejected, newest first");

        assert(![call(tools, @"axefx_audio_analyze", @{@"clip_id":@"nope"})[@"ok"] boolValue]);
        assert(![call(tools, @"axefx_audio_analyze", @{@"clip_id":reference[@"id"], @"duration":@0})[@"ok"] boolValue]);
        assert(![call(tools, @"axefx_audio_analyze", @{@"clip_id":reference[@"id"], @"extra":@1})[@"ok"] boolValue]);
        NSDictionary *one = call(tools, @"axefx_audio_analyze", @{@"clip_id":reference[@"id"]});
        assert([one[@"ok"] boolValue] && one[@"report"][@"measurements"][@"spectral_centroid_hz"]);
        NSDictionary *cmp = call(tools, @"axefx_audio_compare", @{@"reference_id":reference[@"id"], @"current_id":current[@"id"]});
        assert([cmp[@"ok"] boolValue] && [cmp[@"observations"] count] > 0 && [cmp[@"warnings"] count] == 0);
        BOOL darker = NO;
        for (NSDictionary *o in cmp[@"observations"]) if ([o[@"kind"] isEqual:@"spectrum"] && [o[@"delta_db"] doubleValue] < 0) darker = YES;
        assert(darker);
        NSDictionary *swapped = call(tools, @"axefx_audio_compare", @{@"reference_id":current[@"id"], @"current_id":reference[@"id"]});
        assert([swapped[@"warnings"] count] == 2);
        NSString *json = [[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:@[list, one, cmp] options:0 error:nil] encoding:NSUTF8StringEncoding];
        if ([json containsString:root] || [json containsString:@"/Audio/"]) { puts(json.UTF8String); assert(0); }
        for (NSDictionary *c in list[@"clips"]) assert(!c[@"file"]);
        puts("PASS: analyze/compare validation, darker take detected, role warnings, no file paths in results");

        [clips setRole:@"reference" forClip:current[@"id"]];
        assert([[clips clipWithID:current[@"id"]][@"role"] isEqual:@"reference"]);
        [clips removeClip:current[@"id"]];
        assert(![clips clipWithID:current[@"id"]] && [clips.clips count] == 1);
        AxeAudioClips *reloaded = [[AxeAudioClips alloc] initWithDirectory:[NSURL fileURLWithPath:[root stringByAppendingPathComponent:@"Audio"]]];
        assert([reloaded.clips count] == 1 && [reloaded.clips[0][@"id"] isEqual:reference[@"id"]]);
        puts("PASS: role change, delete and persisted library reload");
        [NSFileManager.defaultManager removeItemAtPath:root error:nil];
    }
    return 0;
}
