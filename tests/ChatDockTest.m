// Regression: docked chat-panel geometry and window behaviour, without Axe-Edit.
#define AXE_SIDEBAR_SELF_TEST 1
#import "ChatDock.m"
#include <assert.h>

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSRect screen = NSMakeRect(0, 0, 2000, 1200);
        // Pure geometry: flush right, same height/origin; left when no room.
        for (CGFloat x = 0; x <= 1600; x += 100) {
            for (CGFloat width = 100; width <= 900; width += 100) {
                NSRect editor = NSMakeRect(x, 100, 1000, 750);
                NSRect frame = dockedFrame(editor, width, screen);
                assert(NSWidth(frame) == clampWidth(width) && NSWidth(frame) >= 260 && NSWidth(frame) <= 700);
                assert(NSMinY(frame) == NSMinY(editor) && NSHeight(frame) == NSHeight(editor));
                BOOL right = NSMinX(frame) == NSMaxX(editor), left = NSMaxX(frame) == NSMinX(editor);
                assert(right || left);
                if (NSMaxX(editor) + NSWidth(frame) <= NSMaxX(screen)) assert(right);
            }
        }
        puts("PASS: docked frame is flush with the editor, same height, clamped width, flips left at the screen edge");
        // Live windows: the panel is a child of the editor and follows resizes.
        [NSUserDefaults.standardUserDefaults removeObjectForKey:kDetachedKey];
        NSWindow *editor = [[NSWindow alloc] initWithContentRect:NSMakeRect(100, 200, 800, 600)
            styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskResizable backing:NSBackingStoreBuffered defer:NO];
        editor.title = @"Axe-Edit III";
        [editor orderFront:nil];
        AxeChatDock *test = [[AxeChatDock alloc] initWithEditorWindow:editor];
        assert(test.panel.parentWindow == editor && !test.detached);
        for (CGFloat height = 400; height <= 900; height += 100) {
            [editor setFrame:NSMakeRect(100, 150, height * 4 / 3, height) display:NO];
            assert(NSHeight(test.panel.frame) == NSHeight(editor.frame));
            assert(NSMinY(test.panel.frame) == NSMinY(editor.frame));
            assert(NSMinX(test.panel.frame) == NSMaxX(editor.frame) || NSMaxX(test.panel.frame) == NSMinX(editor.frame));
        }
        puts("PASS: docked panel stays a child window and follows editor resizes");
        [test toggleMode:nil];
        assert(test.detached && test.panel.parentWindow == nil);
        [editor setFrame:NSMakeRect(300, 300, 700, 525) display:NO];
        assert(NSHeight(test.panel.frame) != 525 || NSMinX(test.panel.frame) != NSMaxX(editor.frame));
        [test toggleMode:nil];
        assert(!test.detached && test.panel.parentWindow == editor && NSHeight(test.panel.frame) == 525);
        puts("PASS: detach releases the panel; dock re-attaches and re-snaps");
        [NSUserDefaults.standardUserDefaults removeObjectForKey:kDetachedKey];
    }
    return 0;
}
