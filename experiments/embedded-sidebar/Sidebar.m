// Embedded AppKit sidebar in a separately signed Axe-Edit copy.
// Local-agent chat is isolated from device control and MIDI.
#import <Cocoa/Cocoa.h>
#ifndef AXE_SIDEBAR_SELF_TEST
#import "ChatPanel.h"
#endif
#import <objc/runtime.h>
#include <dlfcn.h>

static const void *hostKey = &hostKey;
typedef NSRect (*FrameRectFunction)(id, SEL, NSRect);
static NSMutableDictionary<NSString *, NSValue *> *originalFrameRects;
static NSMutableDictionary<NSString *, NSValue *> *originalConstraints;
static void *editorImageBase;
static IMP nativeConstrainRect;

static void record(NSString *message) {
    NSString *path = [NSHomeDirectory() stringByAppendingPathComponent:
        @"Library/Logs/AxeEditSidebarProof.log"];
    NSString *line = [NSString stringWithFormat:@"%@ %@\n", [NSDate date], message];
    NSData *data = [line dataUsingEncoding:NSUTF8StringEncoding];
    NSFileHandle *file = [NSFileHandle fileHandleForWritingAtPath:path];
    if (!file) [data writeToFile:path atomically:YES];
    else { [file seekToEndOfFile]; [file writeData:data]; [file closeFile]; }
}

@interface AxeSidebarHost : NSView
@property(nonatomic, strong) NSView *editor;
@property(nonatomic, strong) NSView *sidebar;
@property(nonatomic, strong) NSButton *toggle;
@property(nonatomic) BOOL collapsed;
@property(nonatomic) BOOL layingOut;
@property(nonatomic) CGFloat editorAspect;
@property(nonatomic) FrameRectFunction originalFrameRect;
@end

static NSSize hostSize(AxeSidebarHost *host, NSSize proposed) {
    CGFloat sidebarWidth = host.collapsed ? 36 : 300;
    CGFloat editorWidth = MAX(400, proposed.width - sidebarWidth);
    return NSMakeSize(editorWidth + sidebarWidth, editorWidth * host.editorAspect);
}

static IMP savedConstraint(id self, SEL selector) {
    for (Class cls = object_getClass(self); cls; cls = class_getSuperclass(cls)) {
        NSString *key = [NSString stringWithFormat:@"%@/%@", NSStringFromClass(cls), NSStringFromSelector(selector)];
        NSValue *saved = originalConstraints[key];
        if (saved) return (IMP)saved.pointerValue;
    }
    return NULL;
}

static NSRect sidebarConstrainRect(id self, SEL selector, NSRect frame, NSScreen *screen) {
    AxeSidebarHost *host = objc_getAssociatedObject(self, hostKey);
    if (!host) return ((NSRect (*)(id, SEL, NSRect, NSScreen *))savedConstraint(self, selector))(self, selector, frame, screen);
    CGFloat top = NSMaxY(frame);
    frame.size = hostSize(host, frame.size);
    frame.origin.y = top - frame.size.height;
    // Keep AppKit's screen handling, while replacing JUCE's whole-window
    // aspect constraint with editor-width-plus-sidebar geometry.
    return ((NSRect (*)(id, SEL, NSRect, NSScreen *))nativeConstrainRect)(self, selector, frame, screen);
}

static NSSize sidebarWillResize(id self, SEL selector, NSWindow *window, NSSize proposed) {
    AxeSidebarHost *host = objc_getAssociatedObject(self, hostKey);
    if (host) return hostSize(host, proposed);
    return ((NSSize (*)(id, SEL, NSWindow *, NSSize))savedConstraint(self, selector))(self, selector, window, proposed);
}

// JUCE's peer still submits editor-sized content rectangles to its NSWindow.
// Translate those rectangles to host-sized ones, rather than subtracting the
// sidebar again on each layout notification. Limit the override to this window.
static NSRect sidebarFrameRect(id self, SEL selector, NSRect content) {
    AxeSidebarHost *host = objc_getAssociatedObject(self, hostKey);
    Dl_info caller = {0};
    // AppKit also calls this method while calculating native frame margins.
    // Only JUCE's direct editor-to-window conversion needs the extra width.
    if (host && dladdr(__builtin_return_address(0), &caller) &&
        caller.dli_fbase == editorImageBase)
        content.size.width += host.collapsed ? 36 : 300;
    // Capture the original IMP at installation. AppKit/KVO may add another
    // dynamic subclass later; walking the current superclass would find this
    // override again and recurse (the dragging crash in the v2 experiment).
    FrameRectFunction original = host.originalFrameRect;
    for (Class cls = object_getClass(self); !original && cls; cls = class_getSuperclass(cls)) {
        original = (FrameRectFunction)originalFrameRects[NSStringFromClass(cls)].pointerValue;
    }
    return original(self, selector, content);
}

static void installWindowTranslation(NSWindow *window, AxeSidebarHost *host) {
    Class parent = object_getClass(window);
    if (!nativeConstrainRect) nativeConstrainRect = class_getMethodImplementation(
        NSWindow.class, @selector(constrainFrameRect:toScreen:));
    if (!originalFrameRects) originalFrameRects = [NSMutableDictionary dictionary];
    NSString *name = NSStringFromClass(parent);
    NSValue *saved = originalFrameRects[name];
    if (!saved) {
        Method method = class_getInstanceMethod(parent, @selector(frameRectForContentRect:));
        saved = [NSValue valueWithPointer:(const void *)method_getImplementation(method)];
        originalFrameRects[name] = saved;
        // Preserve the existing isa/KVO chain. Replacing the object's class can
        // invalidate AppKit's observation bookkeeping during drag or teardown.
        class_replaceMethod(parent, @selector(frameRectForContentRect:),
            (IMP)sidebarFrameRect, method_getTypeEncoding(method));
    }
    host.originalFrameRect = (FrameRectFunction)saved.pointerValue;
    objc_setAssociatedObject(window, hostKey, host, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    if (!originalConstraints) originalConstraints = [NSMutableDictionary dictionary];
    for (NSString *selectorName in @[@"constrainFrameRect:toScreen:", @"windowWillResize:toSize:"]) {
        SEL selector = NSSelectorFromString(selectorName);
        NSString *key = [NSString stringWithFormat:@"%@/%@", name, selectorName];
        Method method = class_getInstanceMethod(parent, selector);
        if (method && !originalConstraints[key]) {
            originalConstraints[key] = [NSValue valueWithPointer:(const void *)method_getImplementation(method)];
            class_replaceMethod(parent, selector,
                selector == @selector(constrainFrameRect:toScreen:) ? (IMP)sidebarConstrainRect : (IMP)sidebarWillResize,
                method_getTypeEncoding(method));
        }
    }
}

@implementation AxeSidebarHost
- (BOOL)isFlipped { return NO; }
- (void)resizeSubviewsWithOldSize:(NSSize)oldSize {
    if (self.layingOut) return;
    self.layingOut = YES;
    CGFloat width = self.collapsed ? 36 : 300;
    NSRect bounds = self.bounds;
    self.editor.frame = NSMakeRect(0, 0, MAX(1, bounds.size.width - width), bounds.size.height);
    self.sidebar.frame = NSMakeRect(bounds.size.width - width, 0, width, bounds.size.height);
    self.toggle.frame = NSMakeRect(5, bounds.size.height - 40, width - 10, 28);
    for (NSView *view in self.sidebar.subviews) if (view != self.toggle)
        view.frame = NSMakeRect(0, 0, width, MAX(1, bounds.size.height - 48));
    record([NSString stringWithFormat:@"layout editor=%@ sidebar=%@",
        NSStringFromRect(self.editor.frame), NSStringFromRect(self.sidebar.frame)]);
    self.layingOut = NO;
}
- (void)toggleSidebar:(id)sender {
    self.collapsed = !self.collapsed;
    for (NSView *view in self.sidebar.subviews)
        if (view != self.toggle) view.hidden = self.collapsed;
    self.toggle.title = self.collapsed ? @"‹" : @"Hide sidebar ›";
    // Preserve the current editor dimensions when toggling. Only the host
    // width changes, so reopening cannot make the window taller.
    NSWindow *window = self.window;
    NSRect frame = window.frame;
    CGFloat top = NSMaxY(frame);
    CGFloat width = self.collapsed ? 36 : 300;
    NSRect screen = (window.screen ?: NSScreen.mainScreen).visibleFrame;
    CGFloat editorWidth = MIN(self.editor.frame.size.width, screen.size.width - width - 40);
    frame.size.width = editorWidth + width;
    frame.size.height = editorWidth * self.editorAspect;
    frame.origin.x = MIN(MAX(frame.origin.x, screen.origin.x), NSMaxX(screen) - frame.size.width);
    frame.origin.y = top - frame.size.height;
    [window setFrame:frame display:YES];
    [self resizeSubviewsWithOldSize:self.bounds.size];
    record(self.collapsed ? @"collapsed" : @"expanded");
}
@end

static BOOL attach(void) {
    for (NSWindow *window in NSApp.windows) {
        if (![window.title containsString:@"Axe-Edit III"] || !window.visible ||
            window.contentView.bounds.size.width < 100) continue;
        if ([window.contentView isKindOfClass:AxeSidebarHost.class]) return YES;
        NSView *editor = window.contentView;
        Dl_info editorImage = {0};
        dladdr((void *)class_getMethodImplementation(object_getClass(editor), @selector(mouseDown:)), &editorImage);
        editorImageBase = editorImage.dli_fbase;
        NSRect bounds = editor.bounds;
        AxeSidebarHost *host = [[AxeSidebarHost alloc] initWithFrame:bounds];
        host.editor = editor;
        // The shipped editor constrains its layout to 4:3. A restored native
        // frame can be transiently different while startup layout is running.
        host.editorAspect = 0.75;
        host.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        NSView *sidebar = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 300, bounds.size.height)];
        sidebar.wantsLayer = YES;
        sidebar.layer.backgroundColor = [NSColor colorWithWhite:0.14 alpha:1].CGColor;
        sidebar.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
        host.sidebar = sidebar;
        NSButton *toggle = [NSButton buttonWithTitle:@"Hide sidebar ›" target:host action:@selector(toggleSidebar:)];
        toggle.bezelStyle = NSBezelStyleRounded;
        host.toggle = toggle;
        [sidebar addSubview:toggle];
#ifndef AXE_SIDEBAR_SELF_TEST
        [sidebar addSubview:[[AxeChatPanel alloc] initWithFrame:NSMakeRect(0, 0, 300, bounds.size.height - 48)]];
#endif
        // Retain the original view before replacing contentView; JUCE still owns its peer.
        installWindowTranslation(window, host);
        record([NSString stringWithFormat:@"native aspect=%@ contentAspect=%@",
            NSStringFromSize(window.aspectRatio), NSStringFromSize(window.contentAspectRatio)]);
        window.contentView = host;
        editor.autoresizingMask = NSViewNotSizable;
        [host addSubview:editor];
        [host addSubview:sidebar];
        NSRect screen = (window.screen ?: NSScreen.mainScreen).visibleFrame;
        CGFloat initialWidth = MIN(1500, screen.size.width - 40);
        CGFloat initialHeight = (initialWidth - 300) * host.editorAspect;
        if (initialHeight > screen.size.height - 40) {
            initialHeight = screen.size.height - 40;
            initialWidth = initialHeight / host.editorAspect + 300;
        }
        [window setFrame:NSMakeRect(screen.origin.x + 20,
            NSMaxY(screen) - initialHeight - 20, initialWidth, initialHeight) display:YES];
        [host resizeSubviewsWithOldSize:bounds.size];
        record([NSString stringWithFormat:@"attached window=%@ originalClass=%@",
            window.title, NSStringFromClass(editor.class)]);
        return YES;
    }
    return NO;
}

#ifndef AXE_SIDEBAR_SELF_TEST
__attribute__((constructor)) static void initialize(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        record(@"library loaded");
        __block NSInteger attempts = 0;
        [NSTimer scheduledTimerWithTimeInterval:0.5 repeats:YES block:^(NSTimer *timer) {
            if (attach() || ++attempts >= 60) {
                if (attempts >= 60) record(@"attachment timed out");
                [timer invalidate];
            }
        }];
    });
}
#endif
