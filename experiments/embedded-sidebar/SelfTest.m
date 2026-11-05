// Regression: method dispatch must survive a subclass added after attachment.
#define AXE_SIDEBAR_SELF_TEST 1
#import "Sidebar.m"
#include <assert.h>

@interface ProofObserver : NSObject
@end
@implementation ProofObserver
- (void)observeValueForKeyPath:(NSString *)keyPath ofObject:(id)object
                       change:(NSDictionary *)change context:(void *)context {}
@end

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 1000, 600)
            styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:YES];
        NSRect content = NSMakeRect(10, 20, 800, 600);
        NSRect baseline = [window frameRectForContentRect:content];
        Dl_info testImage = {0};
        assert(dladdr((void *)main, &testImage));
        editorImageBase = testImage.dli_fbase;
        AxeSidebarHost *host = [[AxeSidebarHost alloc] initWithFrame:NSMakeRect(0, 0, 1100, 600)];
        host.editorAspect = 0.75;
        installWindowTranslation(window, host);
        ProofObserver *observer = [ProofObserver new];
        [window addObserver:observer forKeyPath:@"title" options:NSKeyValueObservingOptionNew context:NULL];
        for (NSInteger i = 0; i < 10000; ++i) {
            host.collapsed = i % 2 == 0;
            NSRect result = [window frameRectForContentRect:content];
            assert(fabs(result.size.width - baseline.size.width - (host.collapsed ? 36 : 300)) < 0.01);
            assert(fabs(result.size.height - baseline.size.height) < 0.01);
            assert(NSEqualPoints(result.origin, baseline.origin));
        }
        puts("PASS: 10000 frame translations with an additional runtime subclass; no recursion, height or origin drift");
        [window removeObserver:observer forKeyPath:@"title"];
        // AppKit's internal geometry queries must not acquire fake margins.
        host.collapsed = NO;
        [window setContentSize:NSMakeSize(1100, 600)];
        assert(fabs(window.contentView.bounds.size.width - 1100) < 0.01);
        assert(fabs(window.contentView.bounds.size.height - 600) < 0.01);
        puts("PASS: native AppKit content sizing has no sidebar margin inflation");
        for (CGFloat width = 700; width <= 1700; width += 100) {
            for (NSInteger collapsed = 0; collapsed < 2; ++collapsed) {
                host.collapsed = collapsed;
                NSSize size = hostSize(host, NSMakeSize(width, 999));
                assert(fabs(size.width - width) < 0.01);
                assert(fabs(size.height - (width - (collapsed ? 36 : 300)) * 0.75) < 0.01);
            }
        }
        puts("PASS: expanded/collapsed geometry preserves the editor aspect across 11 widths");
    }
    return 0;
}
