#import "AudioTools.h"
#import "AudioAnalysis.h"
#import "AudioClips.h"
#include <cmath>

static NSDictionary *failure(NSString *message) { return @{@"ok":@NO, @"error":message}; }

@implementation AxeAudioTools
+ (NSArray *)specifications {
    NSDictionary *clip = @{@"type":@"string", @"description":@"Clip id from axefx_audio_list"};
    NSDictionary *start = @{@"type":@"number", @"minimum":@0, @"description":@"Excerpt start in seconds (default 0)"};
    NSDictionary *duration = @{@"type":@"number", @"exclusiveMinimum":@0, @"maximum":@60, @"description":@"Excerpt length in seconds (default 30, max 60)"};
    NSDictionary *(^tool)(NSString *, NSString *, NSDictionary *, NSArray *) = ^(NSString *name, NSString *text, NSDictionary *properties, NSArray *required) {
        return @{@"name":name, @"description":text,
            @"inputSchema":@{@"type":@"object", @"properties":properties, @"required":required, @"additionalProperties":@NO}};
    };
    return @[
        tool(@"axefx_audio_list", @"List the user's local audio clips: id, name, role (reference = the tone to match, current = a recording of the current Axe-Fx tone), duration and source. Read-only.", @{}, @[]),
        tool(@"axefx_audio_analyze", @"Measure one clip locally: level, crest, clipping, spectral centroid, band energy, third-octave spectrum, tilt, low/high rolloff, flatness (saturation proxy), echo spacing and decay. Numeric observations only; it does not identify gear or settings. Read-only.",
             @{@"clip_id":clip, @"start":start, @"duration":duration}, @[@"clip_id"]),
        tool(@"axefx_audio_compare", @"Compare a reference clip (the tone to match) with a current-tone clip: level-normalised third-octave deltas (current minus reference) and ranked observations such as excess presence, darker tilt, lower high rolloff, more compression or a missing echo. Map observations to writable parameters of the inspected blocks. Read-only.",
             @{@"reference_id":clip, @"current_id":clip, @"start":start, @"duration":duration}, @[@"reference_id", @"current_id"]),
    ];
}
static BOOL number(id value, double low, double high, double *out) {
    if (!value) return YES;
    if (![value isKindOfClass:NSNumber.class] || CFGetTypeID((__bridge CFTypeRef)value) == CFBooleanGetTypeID()) return NO;
    double v = [value doubleValue];
    if (!std::isfinite(v) || v < low || v > high) return NO;
    *out = v; return YES;
}
+ (void)call:(NSString *)name arguments:(NSDictionary *)args clips:(AxeAudioClips *)clips reply:(void (^)(NSDictionary *))reply {
    if ([name isEqual:@"axefx_audio_list"]) {
        NSArray *list = clips.clips;
        reply(@{@"ok":@YES, @"clips":list, @"hint":list.count ? @"Use a reference clip and a current clip with axefx_audio_compare."
            : @"No clips yet. Ask the user to import a reference file and record the current tone in the Audio section of the panel."});
        return;
    }
    double start = 0, duration = 30;
    if (!number(args[@"start"], 0, 86400, &start) || !number(args[@"duration"], 0.25, 60, &duration)) { reply(failure(@"Invalid start or duration")); return; }
    NSMutableArray *ids = [NSMutableArray array];
    for (NSString *key in [name isEqual:@"axefx_audio_compare"] ? @[@"reference_id", @"current_id"] : @[@"clip_id"]) {
        if (![args[key] isKindOfClass:NSString.class] || ![clips clipWithID:args[key]]) { reply(failure([NSString stringWithFormat:@"Unknown %@; call axefx_audio_list", key])); return; }
        [ids addObject:args[key]];
    }
    NSMutableArray *files = [NSMutableArray array], *meta = [NSMutableArray array];
    for (NSString *identifier in ids) { [files addObject:[clips fileForClip:identifier]]; [meta addObject:[clips clipWithID:identifier]]; }
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        NSMutableArray *reports = [NSMutableArray array]; NSString *why = nil;
        for (NSURL *file in files) {
            NSDictionary *report = AxeAudioAnalyze(file, start, duration, &why);
            if (!report) break;
            [reports addObject:report];
        }
        NSDictionary *result;
        if (reports.count != files.count) result = failure(why ?: @"Analysis failed");
        else if (reports.count == 1) result = @{@"ok":@YES, @"clip":meta[0], @"report":reports[0]};
        else {
            NSMutableDictionary *comparison = [AxeAudioCompare(reports[0], reports[1]) mutableCopy];
            comparison[@"ok"] = @YES; comparison[@"reference"] = meta[0]; comparison[@"current"] = meta[1];
            NSMutableArray *warnings = [NSMutableArray array];
            if (![meta[0][@"role"] isEqual:@"reference"]) [warnings addObject:@"The first clip is not marked as a reference."];
            if (![meta[1][@"role"] isEqual:@"current"]) [warnings addObject:@"The second clip is not marked as a current-tone recording."];
            comparison[@"warnings"] = warnings;
            comparison[@"limitations"] = reports[0][@"limitations"];
            result = comparison;
        }
        dispatch_async(dispatch_get_main_queue(), ^{ reply(result); });
    });
}
@end
