// Docked AppKit chat panel for a separately signed Axe-Edit copy.
// Axe-Edit's own window is left untouched: its JUCE peer drives live resizing
// with a fixed 4:3 constrainer, so the chat lives in a separate panel that is
// attached as a child window flush against the editor's right edge (or left,
// when the screen has no room). It can be detached into a free window.
// Local-agent chat is isolated from device control and MIDI.
#import <Cocoa/Cocoa.h>
#ifndef AXE_SIDEBAR_SELF_TEST
#import "ChatPanel.h"
#import "EditorTools.h"
#import "NativeBridge.h"
#endif

static NSString *const kWidthKey = @"AxeChatPanelWidth";
static NSString *const kDetachedKey = @"AxeChatPanelDetached";
static NSString *const kDetachedFrameKey = @"AxeChatPanelDetachedFrame";
static const CGFloat kMinWidth = 260, kMaxWidth = 700, kDefaultWidth = 360;

static void record(NSString *message) {
    NSString *path = [NSHomeDirectory() stringByAppendingPathComponent:
        @"Library/Logs/AxeEditSidebarProof.log"];
    NSString *line = [NSString stringWithFormat:@"%@ %@\n", [NSDate date], message];
    NSData *data = [line dataUsingEncoding:NSUTF8StringEncoding];
    NSFileHandle *file = [NSFileHandle fileHandleForWritingAtPath:path];
    if (!file) [data writeToFile:path atomically:YES];
    else { [file seekToEndOfFile]; [file writeData:data]; [file closeFile]; }
}

static CGFloat clampWidth(CGFloat width) { return MAX(kMinWidth, MIN(kMaxWidth, width)); }

// Frame for the docked panel: same height and vertical position as the editor
// window, flush to its right edge, or to its left edge when the right side
// would leave the screen. Pure, for the self-test.
static NSRect dockedFrame(NSRect editor, CGFloat width, NSRect screen) {
    width = clampWidth(width);
    NSRect frame = NSMakeRect(NSMaxX(editor), NSMinY(editor), width, NSHeight(editor));
    if (NSMaxX(frame) > NSMaxX(screen) && NSMinX(editor) - width >= NSMinX(screen))
        frame.origin.x = NSMinX(editor) - width;
    return frame;
}

@interface AxeChatDock : NSObject <NSWindowDelegate>
@property(nonatomic, weak) NSWindow *editorWindow;
@property(nonatomic, strong) NSPanel *panel;
@property(nonatomic, strong) NSButton *modeButton;
@property(nonatomic) BOOL detached;
@property(nonatomic) BOOL placing;
@end

@implementation AxeChatDock
- (instancetype)initWithEditorWindow:(NSWindow *)editorWindow {
    if (!(self = [super init])) return nil;
    self.editorWindow = editorWindow;
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    double width = [defaults doubleForKey:kWidthKey];
    NSRect initial = NSMakeRect(0, 0, width > 0 ? clampWidth(width) : kDefaultWidth, NSHeight(editorWindow.frame));
    NSPanel *panel = [[NSPanel alloc] initWithContentRect:initial
        styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskResizable | NSWindowStyleMaskMiniaturizable
        backing:NSBackingStoreBuffered defer:NO];
    panel.title = @"Tone Assistant";
    panel.floatingPanel = NO;               // ordinary level; the child link keeps it beside the editor
    panel.hidesOnDeactivate = NO;
    panel.becomesKeyOnlyIfNeeded = NO;      // the chat composer needs keyboard focus
    panel.releasedWhenClosed = NO;
    panel.minSize = NSMakeSize(kMinWidth, 300);
    panel.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
    panel.delegate = self;
    self.panel = panel;

    NSView *content = panel.contentView;
    content.wantsLayer = YES;
    content.layer.backgroundColor = [NSColor colorWithWhite:0.14 alpha:1].CGColor;
    // Detach/Dock lives as a small icon on the right of the title bar.
    NSButton *mode = [NSButton buttonWithImage:[NSImage imageWithSystemSymbolName:@"rectangle.portrait.on.rectangle.portrait"
        accessibilityDescription:@"Detach"] target:self action:@selector(toggleMode:)];
    mode.bordered = NO;
    mode.frame = NSMakeRect(0, 0, 28, 22);
    NSView *holder = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 36, 22)];
    [holder addSubview:mode];
    NSTitlebarAccessoryViewController *accessory = [NSTitlebarAccessoryViewController new];
    accessory.view = holder;
    accessory.layoutAttribute = NSLayoutAttributeRight;
    [panel addTitlebarAccessoryViewController:accessory];
    self.modeButton = mode;
#ifndef AXE_SIDEBAR_SELF_TEST
    NSView *chat = [[AxeChatPanel alloc] initWithFrame:content.bounds];
    chat.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [content addSubview:chat];
#endif
    NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
    for (NSNotificationName name in @[NSWindowDidResizeNotification, NSWindowDidMoveNotification,
                                      NSWindowDidChangeScreenNotification])
        [center addObserver:self selector:@selector(editorMoved:) name:name object:editorWindow];
    [center addObserver:self selector:@selector(editorClosing:) name:NSWindowWillCloseNotification object:editorWindow];
    self.detached = [defaults boolForKey:kDetachedKey];
    [self applyMode];
    return self;
}
- (NSRect)screenFrame {
    return (self.editorWindow.screen ?: NSScreen.mainScreen).visibleFrame;
}
// Snap beside the editor. Child windows move with their parent automatically;
// height and side are re-applied after editor resizes and screen changes.
- (void)snap {
    if (self.detached || !self.editorWindow) return;
    self.placing = YES;
    [self.panel setFrame:dockedFrame(self.editorWindow.frame, NSWidth(self.panel.frame), self.screenFrame) display:YES];
    self.placing = NO;
}
- (void)applyMode {
    NSWindow *editor = self.editorWindow;
    if (self.detached) {
        [editor removeChildWindow:self.panel];
        NSString *saved = [NSUserDefaults.standardUserDefaults stringForKey:kDetachedFrameKey];
        NSRect frame = saved ? NSRectFromString(saved) : NSZeroRect;
        if (NSIsEmptyRect(frame) || !NSIntersectsRect(frame, self.screenFrame))
            frame = NSOffsetRect(dockedFrame(editor.frame, NSWidth(self.panel.frame), self.screenFrame), 24, -24);
        [self.panel setFrame:frame display:YES];
        self.modeButton.image = [NSImage imageWithSystemSymbolName:@"rectangle.portrait.inset.filled" accessibilityDescription:@"Dock to Axe-Edit"];
        self.modeButton.toolTip = @"Dock to Axe-Edit";
        [self.panel orderFront:nil];
    } else {
        [self snap];
        if (self.panel.parentWindow != editor) [editor addChildWindow:self.panel ordered:NSWindowAbove];
        self.modeButton.image = [NSImage imageWithSystemSymbolName:@"rectangle.portrait.on.rectangle.portrait" accessibilityDescription:@"Detach window"];
        self.modeButton.toolTip = @"Detach into a separate window";
    }
    record(self.detached ? @"chat panel detached" : @"chat panel docked");
}
- (void)toggleMode:(id)sender {
    self.detached = !self.detached;
    [NSUserDefaults.standardUserDefaults setBool:self.detached forKey:kDetachedKey];
    [self applyMode];
}
- (void)editorMoved:(NSNotification *)note { [self snap]; }
- (void)editorClosing:(NSNotification *)note { [self.panel orderOut:nil]; }
// Docked: the user may change only the width; height and edge follow the editor.
- (NSSize)windowWillResize:(NSWindow *)window toSize:(NSSize)size {
    if (!self.detached && !self.placing) size.height = NSHeight(self.editorWindow.frame);
    size.width = clampWidth(size.width);
    return size;
}
- (void)windowDidResize:(NSNotification *)note {
    [NSUserDefaults.standardUserDefaults setDouble:NSWidth(self.panel.frame) forKey:kWidthKey];
    if (!self.detached && !self.placing && !self.panel.inLiveResize) [self snap];
}
- (void)windowDidEndLiveResize:(NSNotification *)note { [self snap]; [self rememberDetachedFrame]; }
- (void)windowDidMove:(NSNotification *)note { [self rememberDetachedFrame]; }
- (void)rememberDetachedFrame {
    if (self.detached)
        [NSUserDefaults.standardUserDefaults setObject:NSStringFromRect(self.panel.frame) forKey:kDetachedFrameKey];
}
@end

static AxeChatDock *dock;

static BOOL attach(void) {
    for (NSWindow *window in NSApp.windows) {
        if (![window.title containsString:@"Axe-Edit III"] || !window.visible ||
            window.contentView.bounds.size.width < 100 || [window isKindOfClass:NSPanel.class]) continue;
        if (dock.editorWindow == window) return YES;
#ifndef AXE_SIDEBAR_SELF_TEST
        [AxeEditorTools shared].editor = window.contentView;
        (void)[AxeNativeBridge shared]; // verify the executable once, at attach
#endif
        dock = [[AxeChatDock alloc] initWithEditorWindow:window];
        record([NSString stringWithFormat:@"attached chat panel to window=%@ editorClass=%@",
            window.title, NSStringFromClass(window.contentView.class)]);
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
