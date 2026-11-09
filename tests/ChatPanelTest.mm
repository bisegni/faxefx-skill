#import "ChatPanel.mm"
#include <assert.h>
#import <AVFoundation/AVFoundation.h>
#import "AudioClips.h"
#import "AudioStrip.h"

static NSURL *toneFile(NSURL *dir, NSString *name) {
    NSURL *url = [dir URLByAppendingPathComponent:name];
    AVAudioFormat *format = [[AVAudioFormat alloc] initStandardFormatWithSampleRate:48000 channels:1];
    AVAudioFile *file = [[AVAudioFile alloc] initForWriting:url settings:format.settings error:nil];
    AVAudioPCMBuffer *buffer = [[AVAudioPCMBuffer alloc] initWithPCMFormat:format frameCapacity:48000];
    buffer.frameLength = 48000;
    for (int i = 0; i < 48000; ++i) buffer.floatChannelData[0][i] = 0.3f * sinf(2 * M_PI * 220 * i / 48000.f);
    [file writeFromBuffer:buffer error:nil];
    return url;
}
static void spin(void) { [NSRunLoop.mainRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.2]]; }
static NSButton *trashOfRow(NSView *list, NSUInteger row) { return (NSButton *)list.subviews[row].subviews[4]; }
int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSDictionary *body = @{NSFontAttributeName:[NSFont systemFontOfSize:13]};
        NSAttributedString *rendered = renderFencedMarkdown(@"Before\n```json\n  {\"gain\": 3}\n```\nAfter", body);
        assert([rendered.string isEqual:@"Before\njson\n  {\"gain\": 3}\nAfter\n"]);
        NSUInteger codeIndex = [rendered.string rangeOfString:@"  {"].location;
        NSParagraphStyle *codeStyle = [rendered attribute:NSParagraphStyleAttributeName atIndex:codeIndex effectiveRange:NULL];
        assert(codeStyle.textBlocks.count == 1);
        assert(![rendered attribute:NSParagraphStyleAttributeName atIndex:rendered.length - 2 effectiveRange:NULL]);
        NSAttributedString *streaming = renderFencedMarkdown(@"~~~\n  keep indentation\n```", body);
        assert([streaming.string isEqual:@"  keep indentation\n```\n"]);
        assert([renderFencedMarkdown(@"Use ``` inline", body).string isEqual:@"Use ``` inline\n"]);
        assert([renderFencedMarkdown(@"````\n```\n````", body).string isEqual:@"```\n"]);
        puts("PASS: fenced code styling, indentation, matching fences and unfinished streaming blocks");
        AxeChatPanel *panel = [[AxeChatPanel alloc] initWithFrame:NSMakeRect(0, 0, 300, 852)];
        for (NSNumber *height in @[@852, @550, @300, @852]) {
            panel.frame = NSMakeRect(0, 0, 300, height.doubleValue);
            [panel resizeSubviewsWithOldSize:NSZeroSize];
            assert(panel.composer.frame.size.width > 200);
            assert(panel.composer.frame.size.height > 20);
            assert(panel.history.frame.size.width > 200);
            assert(panel.composerScroll.frame.origin.y >= NSMaxY(panel.historyScroll.frame));
            assert(panel.historyScroll.contentView.bounds.origin.x == 0);
            panel.composer.string = @"Native composer test";
            assert([panel.composer.string isEqual:@"Native composer test"]);
        }
        // The Audio strip collapses to one row and expands below the status line.
        panel.frame = NSMakeRect(0, 0, 360, 852);
        panel.audioStrip.expanded = NO; [panel resizeSubviewsWithOldSize:NSZeroSize];
        CGFloat collapsed = panel.audioStrip.frame.size.height;
        panel.audioStrip.expanded = YES; [panel resizeSubviewsWithOldSize:NSZeroSize];
        assert(collapsed == 28 && panel.audioStrip.frame.size.height > collapsed);
        assert(NSMaxY(panel.audioStrip.frame) <= NSMinY(panel.historyScroll.frame));
        panel.audioStrip.expanded = NO;
        puts("PASS: audio strip collapses/expands above the transcript");

        // Deleting a clip from its own trash button must not tear the button down mid-click.
        NSURL *dir = [NSURL fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:NSUUID.UUID.UUIDString] isDirectory:YES];
        [NSFileManager.defaultManager createDirectoryAtURL:dir withIntermediateDirectories:YES attributes:nil error:nil];
        AxeAudioClips *store = [[AxeAudioClips alloc] initWithDirectory:[dir URLByAppendingPathComponent:@"lib"]];
        AxeAudioStrip *strip = [[AxeAudioStrip alloc] initWithFrame:NSMakeRect(0, 0, 300, 200)];
        [strip setValue:store forKey:@"clips"];
        strip.expanded = YES;
        NSString *why = nil;
        assert([store importFile:toneFile(dir, @"a.wav") failure:&why] && [store importFile:toneFile(dir, @"b.wav") failure:&why]);
        spin();
        NSView *list = [strip valueForKey:@"list"];
        assert(list.subviews.count == 2);
        NSURL *doomed = [store fileForClip:list.subviews[0].identifier];
        @autoreleasepool { [trashOfRow(list, 0) performClick:nil]; }
        spin();
        assert(list.subviews.count == 1 && store.clips.count == 1);
        assert(![NSFileManager.defaultManager fileExistsAtPath:doomed.path]);
        [NSFileManager.defaultManager removeItemAtURL:dir error:nil];
        puts("PASS: deleting a clip from its trash button removes it without tearing the button down mid-click");
        puts("PASS: composer and transcript document widths, compact layouts and text retention");
    }
    return 0;
}
