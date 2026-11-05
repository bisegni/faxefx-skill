#import "ChatPanel.mm"
#include <assert.h>
int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
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
        puts("PASS: composer and transcript document widths, compact layouts and text retention");
    }
    return 0;
}
