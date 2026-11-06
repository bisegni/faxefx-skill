#import "ChatPanel.h"
#import "ChatAgent.h"
#import "AudioStrip.h"
#import <QuartzCore/QuartzCore.h>

// Layout metrics (points, flipped coordinates).
static const CGFloat kPad = 12, kHeader = 44, kStatus = 20, kFooter = 22;
static const CGFloat kComposerMin = 44, kComposerMax = 120, kActionSize = 26;

static NSColor *panelBackground(void) { return [NSColor colorWithWhite:0.125 alpha:1]; }
static NSColor *fieldBackground(void) { return [NSColor colorWithWhite:0.095 alpha:1]; }

static NSTextField *label(NSString *text, CGFloat size, NSColor *color) {
    NSTextField *field = [NSTextField labelWithString:text];
    field.font = [NSFont systemFontOfSize:size];
    field.textColor = color;
    field.lineBreakMode = NSLineBreakByTruncatingTail;
    return field;
}

static NSImage *symbol(NSString *name, CGFloat points) {
    NSImage *image = [NSImage imageWithSystemSymbolName:name accessibilityDescription:nil];
    return [image imageWithSymbolConfiguration:[NSImageSymbolConfiguration configurationWithPointSize:points weight:NSFontWeightSemibold]];
}

static void styleScroll(NSScrollView *scroll, NSTextView *view, NSColor *background, NSSize inset) {
    scroll.hasVerticalScroller = YES;
    scroll.autohidesScrollers = YES;
    scroll.borderType = NSNoBorder;
    scroll.drawsBackground = NO;
    view.richText = NO;
    view.drawsBackground = YES;
    view.backgroundColor = background;
    view.textContainerInset = inset;
    view.verticallyResizable = YES; view.horizontallyResizable = NO;
    view.autoresizingMask = NSViewWidthSizable;
    view.textContainer.widthTracksTextView = YES;
    view.maxSize = NSMakeSize(CGFLOAT_MAX, CGFLOAT_MAX);
    scroll.documentView = view;
}

@interface AxeChatPanel () <NSTextViewDelegate>
@property(nonatomic, strong) AxeChatAgent *agent;
@property(nonatomic, strong) NSView *statusDot;
@property(nonatomic, strong) NSPopUpButton *provider;
@property(nonatomic, strong) NSButton *connect;
@property(nonatomic, strong) NSButton *clearButton;
@property(nonatomic, strong) AxeAudioStrip *audioStrip;
@property(nonatomic, strong) NSBox *headerRule;
@property(nonatomic, strong) NSTextField *statusLabel;
@property(nonatomic, strong) NSProgressIndicator *spinner;
@property(nonatomic, strong) NSScrollView *historyScroll;
@property(nonatomic, strong) NSTextView *history;
@property(nonatomic, strong) NSView *composerField;
@property(nonatomic, strong) NSScrollView *composerScroll;
@property(nonatomic, strong) NSTextView *composer;
@property(nonatomic, strong) NSTextField *placeholder;
@property(nonatomic, strong) NSButton *actionButton;
@property(nonatomic, strong) NSTextField *hint;
@end

@implementation AxeChatPanel
- (BOOL)isFlipped { return YES; }
- (instancetype)initWithFrame:(NSRect)frame {
    if ((self = [super initWithFrame:frame])) {
        self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        self.wantsLayer = YES;
        self.layer.backgroundColor = panelBackground().CGColor;
        _agent = [AxeChatAgent new];

        _statusDot = [[NSView alloc] initWithFrame:NSZeroRect];
        _statusDot.wantsLayer = YES;
        _statusDot.layer.cornerRadius = 4;
        _provider = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
        [_provider addItemsWithTitles:@[@"Codex", @"Claude"]];
        _provider.controlSize = NSControlSizeSmall;
        _provider.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
        _provider.accessibilityLabel = @"Local agent";
        _connect = [NSButton buttonWithTitle:@"Connect" target:self action:@selector(connectAgent:)];
        _connect.controlSize = NSControlSizeSmall;
        _connect.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
        NSImage *compose = [NSImage imageWithSystemSymbolName:@"square.and.pencil" accessibilityDescription:@"New chat"];
        _clearButton = [NSButton buttonWithImage:[compose imageWithSymbolConfiguration:[NSImageSymbolConfiguration configurationWithPointSize:13 weight:NSFontWeightMedium]]
            target:self action:@selector(clearChat:)];
        _clearButton.bordered = NO; _clearButton.contentTintColor = NSColor.secondaryLabelColor;
        _clearButton.toolTip = @"New chat (clears this conversation)";
        _audioStrip = [[AxeAudioStrip alloc] initWithFrame:NSMakeRect(0, 0, frame.size.width, 28)];
        __weak AxeChatPanel *weakPanel = self;
        _audioStrip.heightChanged = ^{ [weakPanel resizeSubviewsWithOldSize:weakPanel.bounds.size]; };
        _headerRule = [[NSBox alloc] initWithFrame:NSZeroRect];
        _headerRule.boxType = NSBoxSeparator;
        _statusLabel = label(_agent.status, 11, NSColor.secondaryLabelColor);
        _spinner = [[NSProgressIndicator alloc] initWithFrame:NSZeroRect];
        _spinner.style = NSProgressIndicatorStyleSpinning;
        _spinner.controlSize = NSControlSizeSmall;
        _spinner.displayedWhenStopped = NO;
        _spinner.accessibilityLabel = @"Agent working";

        _historyScroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
        _history = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, MAX(1, frame.size.width - 2 * kPad), 100)];
        _history.editable = NO; _history.selectable = YES;
        _history.accessibilityLabel = @"Conversation";
        styleScroll(_historyScroll, _history, panelBackground(), NSMakeSize(4, 10));

        _composerField = [[NSView alloc] initWithFrame:NSZeroRect];
        _composerField.wantsLayer = YES;
        _composerField.layer.cornerRadius = 10;
        _composerField.layer.backgroundColor = fieldBackground().CGColor;
        _composerField.layer.borderWidth = 1;
        _composerField.layer.borderColor = [NSColor colorWithWhite:1 alpha:0.12].CGColor;
        _composerScroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
        _composer = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, MAX(1, frame.size.width - 2 * kPad), kComposerMin)];
        _composer.font = [NSFont systemFontOfSize:13];
        _composer.textColor = NSColor.labelColor;
        _composer.insertionPointColor = NSColor.controlAccentColor;
        _composer.allowsUndo = YES;
        _composer.delegate = self;
        _composer.accessibilityLabel = @"Message to local agent";
        styleScroll(_composerScroll, _composer, fieldBackground(), NSMakeSize(4, 7));
        _placeholder = label(@"Describe a tone or scene…", 13, NSColor.placeholderTextColor);
        _actionButton = [NSButton buttonWithImage:symbol(@"arrow.up.circle.fill", 20) target:self action:@selector(sendOrStop:)];
        _actionButton.bordered = NO;
        _actionButton.imagePosition = NSImageOnly;
        _hint = label(@"Return to send · Shift-Return for a new line · Preset Save stays manual", 10, NSColor.tertiaryLabelColor);

        for (NSView *view in @[_statusDot, _provider, _connect, _clearButton, _audioStrip, _headerRule, _statusLabel, _spinner, _historyScroll,
                               _composerField, _composerScroll, _placeholder, _actionButton, _hint])
            [self addSubview:view];
        __weak AxeChatPanel *weakSelf = self;
        _agent.changed = ^{ [weakSelf refresh]; };
        [self refresh];
        [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(closing:)
            name:NSApplicationWillTerminateNotification object:nil];
    }
    return self;
}
- (void)closing:(NSNotification *)notification { [self.agent disconnect]; }
- (void)dealloc { [NSNotificationCenter.defaultCenter removeObserver:self]; }

// The composer grows with its text between one and about five lines.
- (CGFloat)composerHeight {
    NSLayoutManager *layout = self.composer.layoutManager;
    [layout ensureLayoutForTextContainer:self.composer.textContainer];
    CGFloat text = NSHeight([layout usedRectForTextContainer:self.composer.textContainer]) + 2 * self.composer.textContainerInset.height;
    return MAX(kComposerMin, MIN(kComposerMax, ceil(text)));
}
- (void)resizeSubviewsWithOldSize:(NSSize)oldSize {
    CGFloat width = self.bounds.size.width, height = self.bounds.size.height;
    CGFloat inner = MAX(1, width - 2 * kPad);
    // Header: status dot, agent picker, connect button.
    self.statusDot.frame = NSMakeRect(kPad + 2, (kHeader - 8) / 2, 8, 8);
    self.connect.frame = NSMakeRect(width - kPad - 92, (kHeader - 24) / 2, 92, 24);
    self.clearButton.frame = NSMakeRect(NSMinX(self.connect.frame) - 30, (kHeader - 22) / 2, 24, 22);
    self.provider.frame = NSMakeRect(kPad + 16, (kHeader - 24) / 2, MAX(80, NSMinX(self.clearButton.frame) - kPad - 22), 24);
    self.headerRule.frame = NSMakeRect(0, kHeader - 1, width, 1);
    // Status row: a spinner leads the text while the agent is working.
    BOOL working = self.agent.busy;
    self.spinner.frame = NSMakeRect(kPad, kHeader + 3, 16, 16);
    CGFloat statusX = working ? kPad + 22 : kPad + 2;
    self.statusLabel.frame = NSMakeRect(statusX, kHeader + 4, MAX(1, width - kPad - statusX), 16);
    // Composer docked at the bottom; transcript fills the rest.
    CGFloat composerHeight = [self composerHeight];
    CGFloat fieldTop = height - kFooter - composerHeight - 4;
    self.composerField.frame = NSMakeRect(kPad, fieldTop, inner, composerHeight);
    self.composerScroll.frame = NSMakeRect(kPad + 6, fieldTop + 1, MAX(1, inner - kActionSize - 16), composerHeight - 2);
    self.placeholder.frame = NSMakeRect(kPad + 6 + 9, fieldTop + 8, MAX(1, inner - kActionSize - 30), 18);
    self.actionButton.frame = NSMakeRect(NSMaxX(self.composerField.frame) - kActionSize - 6,
                                         NSMaxY(self.composerField.frame) - kActionSize - 6, kActionSize, kActionSize);
    self.hint.frame = NSMakeRect(kPad + 2, height - kFooter + 3, inner - 4, 14);
    CGFloat stripTop = kHeader + kStatus + 4, stripHeight = [self.audioStrip preferredHeight];
    self.audioStrip.frame = NSMakeRect(0, stripTop, width, stripHeight);
    [self.audioStrip resizeSubviewsWithOldSize:self.audioStrip.bounds.size];
    CGFloat historyTop = stripTop + stripHeight + 4;
    self.historyScroll.frame = NSMakeRect(kPad, historyTop, inner, MAX(10, fieldTop - historyTop - 8));
    for (NSScrollView *scroll in @[self.historyScroll, self.composerScroll]) {
        NSTextView *view = (NSTextView *)scroll.documentView;
        CGFloat documentWidth = MAX(1, scroll.contentSize.width);
        view.minSize = NSMakeSize(documentWidth, scroll.contentSize.height);
        [view setFrameSize:NSMakeSize(documentWidth, MAX(scroll.contentSize.height, view.frame.size.height))];
        view.textContainer.containerSize = NSMakeSize(MAX(1, documentWidth - 2 * view.textContainerInset.width), CGFLOAT_MAX);
        NSPoint offset = scroll.contentView.bounds.origin;
        offset.x = 0;
        [scroll.contentView scrollToPoint:offset];
    }
}

- (NSAttributedString *)transcript {
    NSMutableAttributedString *out = [NSMutableAttributedString new];
    NSMutableParagraphStyle *body = [NSMutableParagraphStyle new];
    body.lineSpacing = 3; body.paragraphSpacing = 16;
    NSMutableParagraphStyle *role = [NSMutableParagraphStyle new];
    role.paragraphSpacing = 3;
    NSMutableParagraphStyle *tool = [body mutableCopy];
    tool.firstLineHeadIndent = 10; tool.headIndent = 10; tool.lineSpacing = 1;
    NSString *assistant = self.provider.titleOfSelectedItem ?: @"Agent";
    // A local welcome when the agent becomes active (shown, never sent or stored).
    if (self.agent.connected && self.agent.ready) {
        [out appendAttributedString:[[NSAttributedString alloc] initWithString:[assistant.uppercaseString stringByAppendingString:@"\n"] attributes:@{
            NSFontAttributeName:[NSFont systemFontOfSize:10 weight:NSFontWeightSemibold], NSKernAttributeName:@0.6,
            NSForegroundColorAttributeName:NSColor.secondaryLabelColor, NSParagraphStyleAttributeName:role}]];
        [out appendAttributedString:[[NSAttributedString alloc] initWithString:
            @"Hi! I’m connected to your Axe-Fx through Axe-Edit. Tell me the sound you’re after — a style, a reference player, or what to fix — and I’ll inspect the current preset and adjust amp, drive, cab, delay, reverb and other knobs, bypass and channels per scene. Every change is a normal Axe-Edit edit you can undo with ⌘Z; saving the preset stays up to you.\n" attributes:@{
            NSFontAttributeName:[NSFont systemFontOfSize:13], NSForegroundColorAttributeName:NSColor.labelColor,
            NSParagraphStyleAttributeName:body}]];
    }
    for (NSDictionary *entry in self.agent.messages) {
        BOOL user = [entry[@"role"] isEqual:@"user"], isTool = [entry[@"role"] isEqual:@"tool"];
        NSString *name = user ? @"You" : (isTool ? @"Axe-Fx" : assistant);
        NSColor *nameColor = user ? NSColor.controlAccentColor : (isTool ? NSColor.tertiaryLabelColor : NSColor.secondaryLabelColor);
        [out appendAttributedString:[[NSAttributedString alloc] initWithString:[name.uppercaseString stringByAppendingString:@"\n"] attributes:@{
            NSFontAttributeName:[NSFont systemFontOfSize:10 weight:NSFontWeightSemibold], NSKernAttributeName:@0.6,
            NSForegroundColorAttributeName:nameColor, NSParagraphStyleAttributeName:role}]];
        NSString *text = [entry[@"text"] ?: @"" stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
        NSDictionary *attributes = isTool
            ? @{NSFontAttributeName:[NSFont monospacedSystemFontOfSize:11 weight:NSFontWeightRegular],
                NSForegroundColorAttributeName:NSColor.secondaryLabelColor, NSParagraphStyleAttributeName:tool}
            : @{NSFontAttributeName:[NSFont systemFontOfSize:13], NSForegroundColorAttributeName:NSColor.labelColor,
                NSParagraphStyleAttributeName:body};
        [out appendAttributedString:[[NSAttributedString alloc] initWithString:[text stringByAppendingString:@"\n"] attributes:attributes]];
        if ([entry[@"status"] isEqual:@"interrupted"])
            [out appendAttributedString:[[NSAttributedString alloc] initWithString:@"Interrupted\n" attributes:@{
                NSFontAttributeName:[NSFontManager.sharedFontManager convertFont:[NSFont systemFontOfSize:11] toHaveTrait:NSItalicFontMask],
                NSForegroundColorAttributeName:NSColor.tertiaryLabelColor, NSParagraphStyleAttributeName:body}]];
    }
    // While a reply is pending and nothing has streamed yet, show a typing line.
    NSDictionary *last = self.agent.messages.lastObject;
    if (self.agent.busy && self.agent.ready && ![last[@"role"] isEqual:@"assistant"])
        [out appendAttributedString:[[NSAttributedString alloc] initWithString:[NSString stringWithFormat:@"%@ is working…\n", assistant] attributes:@{
            NSFontAttributeName:[NSFontManager.sharedFontManager convertFont:[NSFont systemFontOfSize:12] toHaveTrait:NSItalicFontMask],
            NSForegroundColorAttributeName:NSColor.tertiaryLabelColor, NSParagraphStyleAttributeName:body}]];
    if (!out.length) {
        NSMutableParagraphStyle *centered = [body mutableCopy];
        centered.alignment = NSTextAlignmentCenter;
        [out appendAttributedString:[[NSAttributedString alloc] initWithString:@"\n\nYour Axe-Fx tone assistant\n" attributes:@{
            NSFontAttributeName:[NSFont systemFontOfSize:15 weight:NSFontWeightSemibold],
            NSForegroundColorAttributeName:NSColor.labelColor, NSParagraphStyleAttributeName:centered}]];
        [out appendAttributedString:[[NSAttributedString alloc] initWithString:
            @"Connect a local agent, then describe the sound you want — “warmer clean for scene 2”, “tighter low end on the lead”. It reads the connected preset through Axe-Edit and makes the edits for you.\n" attributes:@{
            NSFontAttributeName:[NSFont systemFontOfSize:12], NSForegroundColorAttributeName:NSColor.secondaryLabelColor,
            NSParagraphStyleAttributeName:centered}]];
    }
    return out;
}

- (void)refresh {
    BOOL connected = self.agent.connected, busy = self.agent.busy;
    self.provider.enabled = !connected;
    self.connect.title = connected ? @"Disconnect" : @"Connect";
    self.clearButton.enabled = !busy && (self.agent.messages.count > 0 || connected);
    NSColor *dot = !connected ? NSColor.tertiaryLabelColor : (busy ? NSColor.systemOrangeColor : (self.agent.ready ? NSColor.systemGreenColor : NSColor.systemYellowColor));
    self.statusDot.layer.backgroundColor = dot.CGColor;
    if (busy) [self.spinner startAnimation:nil]; else [self.spinner stopAnimation:nil];
    self.statusLabel.stringValue = self.agent.status ?: @"";
    self.statusLabel.toolTip = self.agent.status;
    self.composer.editable = !busy;
    self.actionButton.image = symbol(busy ? @"stop.circle.fill" : @"arrow.up.circle.fill", 20);
    self.actionButton.contentTintColor = busy ? NSColor.systemRedColor : NSColor.controlAccentColor;
    self.actionButton.accessibilityLabel = busy ? @"Stop" : @"Send";
    self.actionButton.toolTip = busy ? @"Stop the agent" : @"Send (Return)";
    [self updateActionAvailability];
    [self.history.textStorage setAttributedString:[self transcript]];
    [self resizeSubviewsWithOldSize:self.bounds.size];
    [self.history scrollRangeToVisible:NSMakeRange(self.history.string.length, 0)];
}
- (void)updateActionAvailability {
    BOOL hasText = [self.composer.string stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet].length > 0;
    self.actionButton.enabled = self.agent.busy || (self.agent.ready && hasText);
    self.placeholder.hidden = self.composer.string.length > 0;
}
- (void)textDidChange:(NSNotification *)notification {
    [self updateActionAvailability];
    [self resizeSubviewsWithOldSize:self.bounds.size];
}
// Return sends; Shift-Return (or Option-Return) inserts a newline.
- (BOOL)textView:(NSTextView *)view doCommandBySelector:(SEL)selector {
    if (view != self.composer || selector != @selector(insertNewline:)) return NO;
    NSEventModifierFlags flags = NSApp.currentEvent.modifierFlags;
    if (flags & (NSEventModifierFlagShift | NSEventModifierFlagOption)) return NO;
    [self sendMessage:nil];
    return YES;
}
- (void)clearChat:(id)sender {
    if (self.agent.busy) return;
    NSAlert *alert = [NSAlert new];
    alert.messageText = @"Clear this conversation?";
    alert.informativeText = @"The agent starts a fresh session. Audio clips are kept.";
    [alert addButtonWithTitle:@"Clear"]; [alert addButtonWithTitle:@"Cancel"];
    alert.buttons[0].hasDestructiveAction = YES;
    void (^done)(NSModalResponse) = ^(NSModalResponse response) {
        if (response == NSAlertFirstButtonReturn) [self.agent clear];
    };
    if (self.window) [alert beginSheetModalForWindow:self.window completionHandler:done];
    else done([alert runModal]);
}
- (void)connectAgent:(id)sender {
    if (self.agent.connected) [self.agent disconnect];
    else [self.agent connectProvider:self.provider.titleOfSelectedItem];
}
- (void)sendOrStop:(id)sender {
    if (self.agent.busy) [self.agent stop]; else [self sendMessage:sender];
}
- (void)sendMessage:(id)sender {
    NSString *text = [self.composer.string stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    if (!text.length) return;
    if (text.length > 12000) { self.statusLabel.stringValue = @"Messages are limited to 12,000 characters."; return; }
    if (!self.agent.ready || self.agent.busy) return;
    self.composer.string = @"";
    [self updateActionAvailability];
    [self.agent send:text];
}
- (void)stopAgent:(id)sender { [self.agent stop]; }
@end
