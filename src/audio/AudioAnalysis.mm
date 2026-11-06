// Port of the former Python worker's measure() (numpy/scipy) to Accelerate,
// extended with third-octave spectrum, tilt, rolloff, flatness, echo and decay
// estimates for comparing a reference guitar tone with the current one.
#import "AudioAnalysis.h"
#import <AVFoundation/AVFoundation.h>
#import <Accelerate/Accelerate.h>
#include <algorithm>
#include <cmath>
#include <vector>

static const double kRate = 24000;          // analysis rate, as in the old worker
static const vDSP_Length kFFT = 4096, kHop = 2048;
static const double kFrameSeconds = 0.010;  // envelope frames

static id num(double value) { return std::isfinite(value) ? @(value) : (id)NSNull.null; }
static double db(double ratio) { return 10 * std::log10(std::max(ratio, 1e-12)); }

// Decode [start, start+duration) to per-channel float arrays at the file rate.
static BOOL decode(NSURL *file, double start, double duration, std::vector<std::vector<float>> &channels,
                   double &rate, NSString **failure) {
    NSError *error = nil;
    AVAudioFile *audio = [[AVAudioFile alloc] initForReading:file commonFormat:AVAudioPCMFormatFloat32 interleaved:NO error:&error];
    if (!audio) { *failure = @"The audio file could not be read"; return NO; }
    AVAudioFormat *format = audio.processingFormat;
    rate = format.sampleRate;
    AVAudioChannelCount count = format.channelCount;
    if (rate < 8000 || rate > 384000 || count < 1 || count > 8) { *failure = @"Unsupported audio format"; return NO; }
    AVAudioFramePosition first = (AVAudioFramePosition)(start * rate);
    if (first >= audio.length) { *failure = @"The excerpt starts after the end of the audio"; return NO; }
    AVAudioFrameCount frames = (AVAudioFrameCount)std::min<double>(duration * rate, (double)(audio.length - first));
    AVAudioPCMBuffer *buffer = [[AVAudioPCMBuffer alloc] initWithPCMFormat:format frameCapacity:frames];
    audio.framePosition = first;
    if (![audio readIntoBuffer:buffer frameCount:frames error:&error]) { *failure = @"The audio file could not be decoded"; return NO; }
    channels.assign(count, std::vector<float>(buffer.frameLength));
    for (AVAudioChannelCount c = 0; c < count; ++c)
        std::copy(buffer.floatChannelData[c], buffer.floatChannelData[c] + buffer.frameLength, channels[c].begin());
    return YES;
}

// Band-limited resampling of the mono mix to 24 kHz.
static std::vector<float> resample(const std::vector<float> &mono, double rate) {
    if (rate == kRate) return mono;
    AVAudioFormat *in = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatFloat32 sampleRate:rate channels:1 interleaved:NO];
    AVAudioFormat *out = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatFloat32 sampleRate:kRate channels:1 interleaved:NO];
    AVAudioConverter *converter = [[AVAudioConverter alloc] initFromFormat:in toFormat:out];
    converter.sampleRateConverterQuality = AVAudioQualityMax;
    AVAudioPCMBuffer *source = [[AVAudioPCMBuffer alloc] initWithPCMFormat:in frameCapacity:(AVAudioFrameCount)mono.size()];
    source.frameLength = (AVAudioFrameCount)mono.size();
    std::copy(mono.begin(), mono.end(), source.floatChannelData[0]);
    AVAudioFrameCount capacity = (AVAudioFrameCount)(mono.size() * kRate / rate) + 1024;
    AVAudioPCMBuffer *target = [[AVAudioPCMBuffer alloc] initWithPCMFormat:out frameCapacity:capacity];
    __block BOOL supplied = NO;
    [converter convertToBuffer:target error:nil withInputFromBlock:^AVAudioBuffer *(AVAudioPacketCount n, AVAudioConverterInputStatus *status) {
        if (supplied) { *status = AVAudioConverterInputStatus_EndOfStream; return nil; }
        supplied = YES; *status = AVAudioConverterInputStatus_HaveData; return source;
    }];
    return std::vector<float>(target.floatChannelData[0], target.floatChannelData[0] + target.frameLength);
}

// Welch power spectrum: Hann window, 4096 points, 50 % overlap (bins 0..N/2).
static std::vector<double> welch(const std::vector<float> &x) {
    std::vector<float> window(kFFT), frame(kFFT), re(kFFT / 2), im(kFFT / 2);
    vDSP_hann_window(window.data(), kFFT, vDSP_HANN_NORM);
    vDSP_DFT_Setup setup = vDSP_DFT_zrop_CreateSetup(nullptr, kFFT, vDSP_DFT_FORWARD);
    std::vector<double> power(kFFT / 2 + 1, 0);
    size_t frames = 0;
    for (size_t offset = 0; frames == 0 || offset + kFFT <= x.size(); offset += kHop) {
        std::fill(frame.begin(), frame.end(), 0.f);
        size_t n = std::min<size_t>(kFFT, x.size() - std::min(offset, x.size()));
        vDSP_vmul(x.data() + offset, 1, window.data(), 1, frame.data(), 1, n);
        DSPSplitComplex split = {re.data(), im.data()};
        vDSP_ctoz((const DSPComplex *)frame.data(), 2, &split, 1, kFFT / 2);
        std::vector<float> outRe(kFFT / 2), outIm(kFFT / 2);
        vDSP_DFT_Execute(setup, re.data(), im.data(), outRe.data(), outIm.data());
        power[0] += (double)outRe[0] * outRe[0];             // DC packed in re[0]
        power[kFFT / 2] += (double)outIm[0] * outIm[0];      // Nyquist packed in im[0]
        for (vDSP_Length k = 1; k < kFFT / 2; ++k) power[k] += (double)outRe[k] * outRe[k] + (double)outIm[k] * outIm[k];
        ++frames;
        if (x.size() < kFFT) break;
    }
    vDSP_DFT_DestroySetup(setup);
    for (double &p : power) p /= frames;
    return power;
}

static double binHz(size_t k) { return k * kRate / kFFT; }
static double energyIn(const std::vector<double> &power, double low, double high) {
    double sum = 0;
    for (size_t k = 0; k < power.size(); ++k) if (binHz(k) >= low && binHz(k) < high) sum += power[k];
    return sum;
}
static double percentile(std::vector<double> values, double p) {
    if (values.empty()) return NAN;
    std::sort(values.begin(), values.end());
    double index = p / 100 * (values.size() - 1);
    size_t lo = (size_t)std::floor(index), hi = std::min(lo + 1, values.size() - 1);
    return values[lo] + (values[hi] - values[lo]) * (index - lo);
}

NSDictionary *AxeAudioAnalyze(NSURL *file, double start, double duration, NSString **failure) {
    __autoreleasing NSString *why = nil;
    if (!failure) failure = &why;
    if (!std::isfinite(start) || start < 0 || !std::isfinite(duration) || duration <= 0) { *failure = @"Invalid excerpt"; return nil; }
    duration = std::min(duration, 60.0);
    std::vector<std::vector<float>> channels; double rate = 0;
    if (!decode(file, start, duration, channels, rate, failure)) return nil;
    size_t n = channels[0].size();
    if (n < rate / 4) { *failure = @"Need at least 0.25 seconds of audio"; return nil; }

    // Multichannel interfaces (e.g. the Axe-Fx III's 8 USB inputs) often carry
    // silent channels: measure and mix only channels within 30 dB of the loudest.
    std::vector<double> channelRMS;
    for (auto &channel : channels) {
        double sum = 0;
        for (float v : channel) {
            if (!std::isfinite(v)) { *failure = @"The audio contains invalid samples"; return nil; }
            sum += (double)v * v;
        }
        channelRMS.push_back(std::sqrt(sum / n));
    }
    double loudest = *std::max_element(channelRMS.begin(), channelRMS.end());
    std::vector<std::vector<float>> active;
    for (size_t c = 0; c < channels.size(); ++c) if (channelRMS[c] >= loudest * 0.0316) active.push_back(std::move(channels[c]));
    channels.swap(active);
    double peak = 0, sumSquares = 0, clipped = 0;
    std::vector<float> mono(n, 0.f);
    for (auto &channel : channels) {
        for (size_t i = 0; i < n; ++i) {
            float v = channel[i];
            peak = std::max(peak, (double)std::fabs(v)); sumSquares += (double)v * v;
            if (std::fabs(v) >= 0.999f) clipped += 1;
            mono[i] += v / channels.size();
        }
    }
    double rms = std::sqrt(sumSquares / (n * channels.size()));
    if (rms <= 1e-7) { *failure = @"The audio is silent"; return nil; }
    id stereo = NSNull.null;
    if (channels.size() >= 2) {
        double ma = 0, mb = 0; for (size_t i = 0; i < n; ++i) { ma += channels[0][i]; mb += channels[1][i]; } ma /= n; mb /= n;
        double sab = 0, saa = 0, sbb = 0;
        for (size_t i = 0; i < n; ++i) { double a = channels[0][i] - ma, b = channels[1][i] - mb; sab += a * b; saa += a * a; sbb += b * b; }
        if (saa > 1e-12 && sbb > 1e-12) stereo = @(sab / std::sqrt(saa * sbb));
    }

    std::vector<float> x = resample(mono, rate);
    std::vector<double> power = welch(x);
    double total = 0, weighted = 0;
    for (size_t k = 1; k < power.size(); ++k) { total += power[k]; weighted += binHz(k) * power[k]; }
    total = std::max(total, 1e-30);

    NSDictionary *bands = @{@"low":@(energyIn(power, 20, 120) / total), @"low_mid":@(energyIn(power, 120, 500) / total),
        @"mid":@(energyIn(power, 500, 2000) / total), @"presence":@(energyIn(power, 2000, 6000) / total),
        @"high":@(energyIn(power, 6000, 12000) / total)};

    // Third-octave spectrum, 20 Hz .. 16 kHz, in dB relative to total power.
    NSMutableArray *third = [NSMutableArray array];
    std::vector<double> centers, levels;
    for (int i = 0; i < 30; ++i) {
        double center = 20 * std::pow(2.0, i / 3.0), low = center / std::pow(2.0, 1.0 / 6), high = center * std::pow(2.0, 1.0 / 6);
        if (high > kRate / 2) break;
        double level = db(energyIn(power, low, high) / total);
        centers.push_back(center); levels.push_back(level);
        [third addObject:@{@"hz":@(std::round(center)), @"db":num(level)}];
    }
    // Spectral tilt: regression of band level on octave position, 100 Hz..8 kHz.
    double sx = 0, sy = 0, sxx = 0, sxy = 0; int m = 0;
    for (size_t i = 0; i < centers.size(); ++i) if (centers[i] >= 100 && centers[i] <= 8000) {
        double o = std::log2(centers[i]); sx += o; sy += levels[i]; sxx += o * o; sxy += o * levels[i]; ++m;
    }
    double tilt = m > 2 ? (m * sxy - sx * sy) / (m * sxx - sx * sx) : NAN;
    // Rolloffs: -12 dB relative to the 500 Hz..2 kHz plateau.
    double plateau = 0; int pc = 0;
    for (size_t i = 0; i < centers.size(); ++i) if (centers[i] >= 500 && centers[i] <= 2000) { plateau += levels[i]; ++pc; }
    plateau = pc ? plateau / pc : NAN;
    double highRolloff = NAN, lowRolloff = NAN;
    for (size_t i = 0; i < centers.size(); ++i) if (centers[i] > 1000) { if (levels[i] < plateau - 12) break; highRolloff = centers[i]; }
    for (size_t i = centers.size(); i-- > 0;) if (centers[i] < 500) { if (levels[i] < plateau - 12) break; lowRolloff = centers[i]; }
    // Spectral flatness (100 Hz..8 kHz): 0 tonal/clean .. 1 noise-like/saturated.
    double logSum = 0, linSum = 0; size_t fc = 0;
    for (size_t k = 0; k < power.size(); ++k) if (binHz(k) >= 100 && binHz(k) <= 8000) {
        logSum += std::log(std::max(power[k], 1e-30)); linSum += power[k]; ++fc;
    }
    double flatness = fc ? std::exp(logSum / fc) / std::max(linSum / fc, 1e-30) : NAN;

    // 10 ms RMS envelope.
    size_t hop = (size_t)(kFrameSeconds * kRate);
    std::vector<double> envelope;
    for (size_t i = 0; i + hop <= x.size(); i += hop) {
        double s = 0; for (size_t j = i; j < i + hop; ++j) s += (double)x[j] * x[j];
        envelope.push_back(std::sqrt(s / hop));
    }
    double p10 = percentile(envelope, 10), p50 = percentile(envelope, 50), p90 = percentile(envelope, 90);

    // Echo: strongest envelope autocorrelation peak between 60 ms and 1.5 s.
    id echo = NSNull.null;
    if (envelope.size() > 200) {
        double mean = 0; for (double v : envelope) mean += v; mean /= envelope.size();
        std::vector<double> e(envelope.size()); for (size_t i = 0; i < e.size(); ++i) e[i] = envelope[i] - mean;
        double zero = 0; for (double v : e) zero += v * v;
        double best = 0; size_t bestLag = 0;
        size_t minLag = (size_t)(0.060 / kFrameSeconds), maxLag = std::min<size_t>((size_t)(1.5 / kFrameSeconds), e.size() / 2);
        std::vector<double> r(maxLag + 2, 0);
        for (size_t lag = minLag - 1; lag <= maxLag + 1 && lag < e.size(); ++lag) {
            double s = 0; for (size_t i = 0; i + lag < e.size(); ++i) s += e[i] * e[i + lag];
            r[lag] = zero > 0 ? s / zero : 0;
        }
        // Repeats show as the earliest strong peak; a later, stronger peak is
        // usually the playing rhythm (note spacing), reported separately.
        double strongest = 0; size_t strongestLag = 0;
        for (size_t lag = minLag; lag <= maxLag; ++lag)
            if (r[lag] > r[lag - 1] && r[lag] >= r[lag + 1] && r[lag] > strongest) { strongest = r[lag]; strongestLag = lag; }
        for (size_t lag = minLag; lag <= maxLag && !bestLag; ++lag)
            if (r[lag] > r[lag - 1] && r[lag] >= r[lag + 1] && r[lag] > 0.25 && r[lag] >= 0.3 * strongest) { best = r[lag]; bestLag = lag; }
        if (bestLag) echo = @{@"delay_ms":@(bestLag * kFrameSeconds * 1000), @"strength":@(best),
            @"rhythm_ms":@(strongestLag * kFrameSeconds * 1000)};
    }
    // Decay: median time for the envelope to fall 20 dB after a peak without a
    // new attack (a proxy for sustain plus reverb/delay tails).
    std::vector<double> falls;
    for (size_t i = 1; i + 1 < envelope.size(); ++i) {
        if (envelope[i] < p50 || envelope[i] < envelope[i - 1] || envelope[i] < envelope[i + 1]) continue;
        double floorLevel = envelope[i] * 0.1, lowest = envelope[i];
        for (size_t j = i + 1; j < envelope.size(); ++j) {
            lowest = std::min(lowest, envelope[j]);
            if (envelope[j] > lowest * 1.5 && envelope[j] > floorLevel) break;   // new attack
            if (envelope[j] <= floorLevel) { falls.push_back((j - i) * kFrameSeconds * 1000); i = j; break; }
        }
    }
    double decay = falls.size() >= 2 ? percentile(falls, 50) : NAN;

    double seconds = (double)n / rate;
    return @{@"schema_version":@"2.0", @"kind":@"audio_analysis",
        @"excerpt":@{@"start_s":@(start), @"duration_s":@(seconds)},
        @"processing":@{@"local_only":@YES, @"source_rate_hz":@(rate), @"active_channels":@(channels.size()), @"analysis_rate_hz":@(kRate)},
        @"measurements":@{@"peak_dbfs":num(20 * std::log10(std::max(peak, 1e-12))), @"rms_dbfs":num(20 * std::log10(rms)),
            @"crest_db":num(20 * std::log10(std::max(peak / rms, 1e-12))), @"clipped_sample_fraction":@(clipped / (n * channels.size())),
            @"spectral_centroid_hz":num(weighted / total), @"band_energy":bands, @"third_octave_db":third,
            @"spectral_tilt_db_per_octave":num(tilt), @"low_rolloff_hz":num(lowRolloff), @"high_rolloff_hz":num(highRolloff),
            @"spectral_flatness":num(flatness), @"envelope_p10_p50_p90":@[num(p10), num(p50), num(p90)],
            @"echo":echo, @"decay_t20_ms":num(decay), @"stereo_correlation":stereo},
        @"rig_identified":@NO,
        @"limitations":@[@"Measurements depend on the content and playing; they do not identify an amp, cabinet or exact settings.",
            @"Use isolated guitar for references: a mix includes other instruments that skew the spectrum.",
            @"Compare the same part played the same way; echo and decay estimates are confused by rhythmic playing."]};
}

static double value(NSDictionary *report, NSString *key) {
    id v = report[@"measurements"][key];
    return [v isKindOfClass:NSNumber.class] ? [v doubleValue] : NAN;
}

NSDictionary *AxeAudioCompare(NSDictionary *reference, NSDictionary *current) {
    NSArray *ref = reference[@"measurements"][@"third_octave_db"], *cur = current[@"measurements"][@"third_octave_db"];
    NSMutableArray *bands = [NSMutableArray array], *observations = [NSMutableArray array];
    std::vector<double> hz, delta;
    for (NSUInteger i = 0; i < MIN(ref.count, cur.count); ++i) {
        id a = ref[i][@"db"], b = cur[i][@"db"];
        if (![a isKindOfClass:NSNumber.class] || ![b isKindOfClass:NSNumber.class]) continue;
        // Ignore bands with almost no energy in both clips.
        if ([a doubleValue] < -60 && [b doubleValue] < -60) continue;
        double d = [b doubleValue] - [a doubleValue];
        hz.push_back([ref[i][@"hz"] doubleValue]); delta.push_back(d);
        [bands addObject:@{@"hz":ref[i][@"hz"], @"delta_db":@(std::round(d * 10) / 10)}];
    }
    // Merge neighbouring bands with the same sign and |delta| >= 2 dB into regions.
    for (size_t i = 0; i < delta.size();) {
        if (std::fabs(delta[i]) < 2) { ++i; continue; }
        size_t j = i; double sum = 0;
        while (j < delta.size() && std::fabs(delta[j]) >= 2 && (delta[j] > 0) == (delta[i] > 0)) sum += delta[j++];
        double mean = sum / (j - i);
        [observations addObject:@{@"kind":@"spectrum", @"low_hz":@(hz[i]), @"high_hz":@(hz[j - 1]), @"delta_db":@(std::round(mean * 10) / 10),
            @"weight":@(std::fabs(mean) * (j - i)),
            @"text":[NSString stringWithFormat:@"Current tone has %.1f dB %@ than the reference around %.0f–%.0f Hz",
                std::fabs(mean), mean > 0 ? @"more" : @"less", hz[i], hz[j - 1]]}];
        i = j;
    }
    auto scalar = [&](NSString *key, NSString *label, NSString *unit, double threshold, double weightScale) {
        double a = value(reference, key), b = value(current, key);
        if (!std::isfinite(a) || !std::isfinite(b) || std::fabs(b - a) < threshold) return;
        [observations addObject:@{@"kind":key, @"reference":@(a), @"current":@(b), @"weight":@(std::fabs(b - a) * weightScale),
            @"text":[NSString stringWithFormat:@"%@: current %.4g%@ vs reference %.4g%@", label, b, unit, a, unit]}];
    };
    scalar(@"spectral_tilt_db_per_octave", @"Spectral tilt (darker is more negative)", @" dB/oct", 0.5, 2);
    scalar(@"high_rolloff_hz", @"High-frequency rolloff", @" Hz", 1, 0.002);
    scalar(@"low_rolloff_hz", @"Low-frequency rolloff", @" Hz", 1, 0.02);
    scalar(@"crest_db", @"Crest factor (lower means more compressed/saturated)", @" dB", 1.5, 1.5);
    scalar(@"spectral_flatness", @"Spectral flatness (higher means more distortion/noise)", @"", 0.03, 60);
    scalar(@"decay_t20_ms", @"Decay after notes (sustain/reverb proxy)", @" ms", 80, 0.02);
    id refEcho = reference[@"measurements"][@"echo"], curEcho = current[@"measurements"][@"echo"];
    BOOL refHas = [refEcho isKindOfClass:NSDictionary.class], curHas = [curEcho isKindOfClass:NSDictionary.class];
    if (refHas != curHas || (refHas && std::fabs([refEcho[@"delay_ms"] doubleValue] - [curEcho[@"delay_ms"] doubleValue]) > 40))
        [observations addObject:@{@"kind":@"echo", @"reference":refEcho, @"current":curEcho, @"weight":@4,
            @"text":refHas && !curHas ? [NSString stringWithFormat:@"Reference shows a repeat about %.0f ms apart that the current tone lacks", [refEcho[@"delay_ms"] doubleValue]]
                  : (!refHas ? [NSString stringWithFormat:@"Current tone shows a repeat about %.0f ms apart that the reference lacks", [curEcho[@"delay_ms"] doubleValue]]
                  : [NSString stringWithFormat:@"Repeat spacing differs: current %.0f ms vs reference %.0f ms", [curEcho[@"delay_ms"] doubleValue], [refEcho[@"delay_ms"] doubleValue]])}];
    [observations sortUsingComparator:^NSComparisonResult(NSDictionary *a, NSDictionary *b) { return [b[@"weight"] compare:a[@"weight"]]; }];
    double overall = 0; for (double d : delta) overall += d * d;
    return @{@"schema_version":@"2.0", @"kind":@"audio_comparison",
        @"spectral_distance_db":@(delta.empty() ? 0 : std::round(std::sqrt(overall / delta.size()) * 10) / 10),
        @"third_octave_delta_db":bands, @"observations":observations,
        @"note":@"Deltas are current minus reference, after level normalisation. Observations describe differences, not settings; map them to the inspected block parameters."};
}
