// Local audio measurements for tone matching (AVFoundation + Accelerate).
// Reports are numeric observations only: they never contain audio or file
// paths, and they do not identify an amp, cab or exact settings.
#import <Foundation/Foundation.h>

/// Measure an excerpt of an audio file (wav/aiff/caf/mp3/m4a/flac...).
/// start/duration are seconds; duration is capped at 60 s. Returns nil with a
/// reason for unreadable, silent, too short or unsupported audio.
NSDictionary *AxeAudioAnalyze(NSURL *file, double start, double duration, NSString **failure);

/// Compare two reports from AxeAudioAnalyze: level-normalised spectral deltas
/// (current minus reference) and ranked observations, phrased as differences
/// rather than parameter changes.
NSDictionary *AxeAudioCompare(NSDictionary *reference, NSDictionary *current);
