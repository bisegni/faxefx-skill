// Synthetic-signal checks for the native audio analysis (no microphone needed).
#import "AudioAnalysis.h"
#import <AVFoundation/AVFoundation.h>
#include <assert.h>
#include <cmath>
#include <random>
#include <vector>

static NSURL *writeClip(NSString *name, const std::vector<float> &samples, double rate, NSDictionary *settings = nil) {
    NSURL *url = [NSURL fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:name]];
    [NSFileManager.defaultManager removeItemAtURL:url error:nil];
    NSDictionary *format = settings ?: @{AVFormatIDKey:@(kAudioFormatLinearPCM), AVSampleRateKey:@(rate), AVNumberOfChannelsKey:@1,
        AVLinearPCMBitDepthKey:@32, AVLinearPCMIsFloatKey:@YES, AVLinearPCMIsNonInterleaved:@NO};
    NSError *error = nil;
    AVAudioFile *file = [[AVAudioFile alloc] initForWriting:url settings:format commonFormat:AVAudioPCMFormatFloat32 interleaved:NO error:&error];
    assert(file);
    AVAudioPCMBuffer *buffer = [[AVAudioPCMBuffer alloc] initWithPCMFormat:file.processingFormat frameCapacity:(AVAudioFrameCount)samples.size()];
    buffer.frameLength = (AVAudioFrameCount)samples.size();
    std::copy(samples.begin(), samples.end(), buffer.floatChannelData[0]);
    assert([file writeFromBuffer:buffer error:&error]);
    return url;
}
static double m(NSDictionary *r, NSString *k) { return [r[@"measurements"][k] doubleValue]; }
static std::vector<float> noise(size_t n, unsigned seed, bool pink) {
    std::mt19937 rng(seed); std::normal_distribution<float> g(0, 1);
    std::vector<float> out(n); double b0 = 0, b1 = 0, b2 = 0;
    for (size_t i = 0; i < n; ++i) {
        float w = g(rng);
        if (pink) { b0 = 0.99765 * b0 + w * 0.0990460; b1 = 0.96300 * b1 + w * 0.2965164; b2 = 0.57000 * b2 + w * 1.0526913;
                    out[i] = (float)(0.05 * (b0 + b1 + b2 + w * 0.1848)); }
        else out[i] = 0.1f * w;
    }
    return out;
}
static std::vector<float> lowpass(std::vector<float> x, double hz, double rate, int passes) {
    double a = std::exp(-2 * M_PI * hz / rate);
    for (int p = 0; p < passes; ++p) { double y = 0; for (float &v : x) { y = (1 - a) * v + a * y; v = (float)y; } }
    return x;
}

int main(void) {
    @autoreleasepool {
        const double rate = 48000; NSString *why = nil;
        // Pure tone: centroid at its frequency, no clipping, crest ~3 dB.
        std::vector<float> sine(rate * 3);
        for (size_t i = 0; i < sine.size(); ++i) sine[i] = 0.5f * std::sin(2 * M_PI * 1000 * i / rate);
        NSDictionary *tone = AxeAudioAnalyze(writeClip(@"axe-sine.caf", sine, rate), 0, 30, &why);
        assert(tone && std::fabs(m(tone, @"spectral_centroid_hz") - 1000) < 60);
        assert(m(tone, @"clipped_sample_fraction") == 0 && std::fabs(m(tone, @"crest_db") - 3.01) < 0.2);
        // Clipped sine: clipping fraction and flatness rise, crest drops.
        std::vector<float> hard(sine); for (float &v : hard) v = std::max(-1.f, std::min(1.f, v * 6));
        NSDictionary *clip = AxeAudioAnalyze(writeClip(@"axe-clip.caf", hard, rate), 0, 30, &why);
        assert(m(clip, @"clipped_sample_fraction") > 0.3 && m(clip, @"crest_db") < m(tone, @"crest_db"));
        assert(m(clip, @"spectral_flatness") > m(tone, @"spectral_flatness"));
        puts("PASS: tone centroid, crest and clipping/saturation measures");
        // Pink noise: roughly -3 dB/oct in power per Hz, i.e. ~flat third-octave bands.
        std::vector<float> pink = noise(rate * 8, 1, true);
        NSDictionary *flat = AxeAudioAnalyze(writeClip(@"axe-pink.caf", pink, rate), 0, 30, &why);
        assert(flat && std::fabs(m(flat, @"spectral_tilt_db_per_octave")) < 1.0);
        // Low-passed copy: darker tilt, earlier high rolloff, positive delta at lows.
        NSDictionary *dark = AxeAudioAnalyze(writeClip(@"axe-dark.caf", lowpass(pink, 1500, rate, 2), rate), 0, 30, &why);
        assert(m(dark, @"spectral_tilt_db_per_octave") < m(flat, @"spectral_tilt_db_per_octave") - 2);
        assert(m(dark, @"high_rolloff_hz") < m(flat, @"high_rolloff_hz"));
        NSDictionary *cmp = AxeAudioCompare(flat, dark);
        NSDictionary *top = [cmp[@"observations"] firstObject];
        assert(top && [cmp[@"spectral_distance_db"] doubleValue] > 3);
        BOOL sawTreble = NO;
        for (NSDictionary *o in cmp[@"observations"]) if ([o[@"kind"] isEqual:@"spectrum"] && [o[@"delta_db"] doubleValue] < -2 && [o[@"high_hz"] doubleValue] >= 4000) sawTreble = YES;
        assert(sawTreble);
        puts("PASS: pink-noise flatness, low-pass tilt/rolloff and ranked comparison");
        // Identical clips compare as (near) zero.
        NSDictionary *same = AxeAudioCompare(flat, flat);
        assert([same[@"spectral_distance_db"] doubleValue] < 0.05 && [same[@"observations"] count] == 0);
        // Echo: plucked bursts every 1.2 s plus a 350 ms repeat at -6 dB.
        std::vector<float> dry(rate * 8, 0.f), burst = noise(rate * 8, 2, false);
        for (size_t start = 0; start + rate * 0.3 < dry.size(); start += (size_t)(rate * 1.2))
            for (size_t i = 0; i < rate * 0.3; ++i) dry[start + i] = burst[start + i] * std::exp(-(double)i / (rate * 0.05)) * 5;
        std::vector<float> wet(dry); size_t lag = (size_t)(0.35 * rate);
        for (size_t i = lag; i < wet.size(); ++i) wet[i] += 0.5f * dry[i - lag];
        NSDictionary *echo = AxeAudioAnalyze(writeClip(@"axe-echo.caf", wet, rate), 0, 30, &why);
        id e = echo[@"measurements"][@"echo"];
        assert([e isKindOfClass:NSDictionary.class] && std::fabs([e[@"delay_ms"] doubleValue] - 350) <= 20);
        NSDictionary *dryReport = AxeAudioAnalyze(writeClip(@"axe-dry.caf", dry, rate), 0, 30, &why);
        id de = dryReport[@"measurements"][@"echo"];
        assert(![de isKindOfClass:NSDictionary.class] || std::fabs([de[@"delay_ms"] doubleValue] - 350) > 40);
        puts("PASS: echo spacing detected on a delayed signal, absent on the dry one");
        // Compressed formats decode (AAC in m4a), excerpts work, invalid input is refused.
        NSDictionary *aac = @{AVFormatIDKey:@(kAudioFormatMPEG4AAC), AVSampleRateKey:@44100, AVNumberOfChannelsKey:@1, AVEncoderBitRateKey:@128000};
        NSDictionary *m4a = AxeAudioAnalyze(writeClip(@"axe-aac.m4a", std::vector<float>(sine.begin(), sine.begin() + 44100 * 2), 44100, aac), 0.5, 1, &why);
        assert(m4a && std::fabs(m(m4a, @"spectral_centroid_hz") - 1000 * 44100 / 48000.0) < 120);
        assert([m4a[@"excerpt"][@"duration_s"] doubleValue] <= 1.01);
        assert(!AxeAudioAnalyze(writeClip(@"axe-silent.caf", std::vector<float>(rate, 0.f), rate), 0, 30, &why) && [why containsString:@"silent"]);
        assert(!AxeAudioAnalyze(writeClip(@"axe-short.caf", std::vector<float>(rate / 10, 0.1f), rate), 0, 30, &why));
        assert(!AxeAudioAnalyze([NSURL fileURLWithPath:@"/nonexistent.wav"], 0, 30, &why));
        NSData *json = [NSJSONSerialization dataWithJSONObject:dark options:0 error:nil];
        assert(json && ![[[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding] containsString:NSTemporaryDirectory()]);
        puts("PASS: AAC decode, excerpt bounds, silence/short/missing rejection, JSON-safe report without paths");
    }
    return 0;
}
