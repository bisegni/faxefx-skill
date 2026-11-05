#import "ChatPanel.h"
#import "ChatAgent.h"

static NSTextField *caption(NSString *text, CGFloat size) {
    NSTextField *field = [NSTextField wrappingLabelWithString:text];
    field.font = [NSFont systemFontOfSize:size];
    field.textColor = NSColor.labelColor;
    return field;
}

@interface AxeChatPanel ()
@property(nonatomic, strong) AxeChatAgent *agent;
@property(nonatomic, strong) NSTextField *titleLabel;
@property(nonatomic, strong) NSTextField *subtitle;
@property(nonatomic, strong) NSPopUpButton *provider;
@property(nonatomic, strong) NSButton *connect;
@property(nonatomic, strong) NSTextField *statusLabel;
@property(nonatomic, strong) NSScrollView *historyScroll;
@property(nonatomic, strong) NSTextView *history;
@property(nonatomic, strong) NSScrollView *composerScroll;
@property(nonatomic, strong) NSTextView *composer;
@property(nonatomic, strong) NSButton *sendButton;
@property(nonatomic, strong) NSButton *stopButton;
@end

@implementation AxeChatPanel
- (BOOL)isFlipped { return YES; }
- (instancetype)initWithFrame:(NSRect)frame {
    if ((self = [super initWithFrame:frame])) {
        self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        _agent = [AxeChatAgent new];
        _titleLabel = caption(@"Tone assistant", 21);
        _subtitle = caption(@"Axe-Fx tone agent · device tools", 11);
        _subtitle.textColor = NSColor.secondaryLabelColor;
        _provider = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
        [_provider addItemsWithTitles:@[@"Codex", @"Claude"]];
        _provider.accessibilityLabel = @"Local agent";
        _connect = [NSButton buttonWithTitle:@"Connect" target:self action:@selector(connectAgent:)];
        _statusLabel = caption(_agent.status, 11);
        _statusLabel.textColor = NSColor.secondaryLabelColor;
        _historyScroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
        _historyScroll.hasVerticalScroller = YES;
        _historyScroll.borderType = NSBezelBorder;
        _history = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, frame.size.width - 32, 100)];
        _history.editable = NO; _history.selectable = YES; _history.richText = NO;
        _history.font = [NSFont systemFontOfSize:13];
        _history.textColor = NSColor.labelColor;
        _history.backgroundColor = [NSColor colorWithWhite:0.105 alpha:1];
        _history.textContainerInset = NSMakeSize(10, 10);
        _history.verticallyResizable = YES; _history.horizontallyResizable = NO;
        _history.autoresizingMask = NSViewWidthSizable;
        _history.textContainer.widthTracksTextView = YES;
        _history.maxSize = NSMakeSize(CGFLOAT_MAX, CGFLOAT_MAX);
        _history.accessibilityLabel = @"Conversation";
        _historyScroll.documentView = _history;
        _composerScroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
        _composerScroll.hasVerticalScroller = YES; _composerScroll.borderType = NSBezelBorder;
        _composer = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, frame.size.width - 32, 80)];
        _composer.richText = NO; _composer.font = [NSFont systemFontOfSize:13];
        _composer.textColor = NSColor.labelColor;
        _composer.backgroundColor = [NSColor colorWithWhite:0.10 alpha:1];
        _composer.textContainerInset = NSMakeSize(8, 8);
        _composer.verticallyResizable = YES; _composer.horizontallyResizable = NO;
        _composer.autoresizingMask = NSViewWidthSizable;
        _composer.textContainer.widthTracksTextView = YES;
        _composer.maxSize = NSMakeSize(CGFLOAT_MAX, CGFLOAT_MAX);
        _composer.accessibilityLabel = @"Message to local agent";
        _composerScroll.documentView = _composer;
        _sendButton = [NSButton buttonWithTitle:@"Send" target:self action:@selector(sendMessage:)];
        _stopButton = [NSButton buttonWithTitle:@"Stop" target:self action:@selector(stopAgent:)];
        for (NSView *view in @[_titleLabel, _subtitle, _provider, _connect, _statusLabel,
            _historyScroll, _composerScroll, _sendButton, _stopButton]) [self addSubview:view];
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
- (void)resizeSubviewsWithOldSize:(NSSize)oldSize {
    CGFloat width = self.bounds.size.width, height = self.bounds.size.height;
    BOOL compact = height < 450;
    self.titleLabel.hidden = compact; self.subtitle.hidden = compact;
    self.titleLabel.frame = NSMakeRect(14, 12, width - 28, 30);
    self.subtitle.frame = NSMakeRect(14, 46, width - 28, 20);
    CGFloat header = compact ? 10 : 76;
    self.provider.frame = NSMakeRect(12, header, width - 126, 28);
    self.connect.frame = NSMakeRect(width - 110, header, 98, 28);
    self.statusLabel.frame = NSMakeRect(14, header + 36, width - 28, 40);
    CGFloat transcriptTop = header + 80, composerHeight = compact ? 54 : 80;
    CGFloat composerTop = height - 48 - composerHeight;
    self.historyScroll.frame = NSMakeRect(12, transcriptTop, width - 24, MAX(10, composerTop - transcriptTop - 10));
    self.composerScroll.frame = NSMakeRect(12, composerTop, width - 24, composerHeight);
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
    self.sendButton.frame = NSMakeRect(width - 92, height - 38, 80, 28);
    self.stopButton.frame = NSMakeRect(12, height - 38, 70, 28);
}
- (void)refresh {
    self.provider.enabled = !self.agent.connected;
    self.connect.title = self.agent.connected ? @"Disconnect" : @"Connect";
    self.sendButton.enabled = self.agent.ready && !self.agent.busy;
    self.stopButton.enabled = self.agent.busy;
    self.composer.editable = !self.agent.busy;
    self.statusLabel.stringValue = self.agent.status;
    NSMutableString *text = [NSMutableString string];
    for (NSDictionary *entry in self.agent.messages) {
        NSString *role = [entry[@"role"] isEqual:@"user"] ? @"You" : ([entry[@"role"] isEqual:@"tool"] ? @"Axe-Fx tool" : self.provider.titleOfSelectedItem);
        [text appendFormat:@"%@\n%@\n", role, entry[@"text"] ?: @""];
        if ([entry[@"status"] isEqual:@"interrupted"]) [text appendString:@"[Interrupted]\n"];
        [text appendString:@"\n"];
    }
    if (!text.length) [text appendString:@"Connect your local agent and describe the guitar tone or scene you want.\n\nThe agent inspects the connected Axe-Fx through Axe-Edit before making supported edits. Preset Save remains manual."];
    self.history.string = text;
    [self resizeSubviewsWithOldSize:self.bounds.size];
    [self.history scrollRangeToVisible:NSMakeRange(self.history.string.length, 0)];
}
- (void)connectAgent:(id)sender {
    if (self.agent.connected) [self.agent disconnect];
    else [self.agent connectProvider:self.provider.titleOfSelectedItem];
}
- (void)sendMessage:(id)sender {
    NSString *text = [self.composer.string stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    if (!text.length || text.length > 12000) { self.statusLabel.stringValue = @"Write a message of 1–12000 characters."; return; }
    if (!self.agent.ready || self.agent.busy) return;
    self.composer.string = @"";
    [self.agent send:text];
}
- (void)stopAgent:(id)sender { [self.agent stop]; }
@end
