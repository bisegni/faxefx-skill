#import "AudioStrip.h"
#import "AudioClips.h"
#import "AudioRecorder.h"
#import <AVFoundation/AVFoundation.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

static NSString *const kExpandedKey = @"AxeAudioStripExpanded";
static NSString *const kDeviceKey = @"AxeAudioInputDevice";
static const CGFloat kPad = 12, kRow = 26, kHeaderRow = 28, kControlsRow = 32, kMessageRow = 18, kMaxVisibleRows = 4;

static NSImage *icon(NSString *name, CGFloat size) {
    NSImage *image = [NSImage imageWithSystemSymbolName:name accessibilityDescription:nil];
    return [image imageWithSymbolConfiguration:[NSImageSymbolConfiguration configurationWithPointSize:size weight:NSFontWeightMedium]];
}
static NSButton *iconButton(NSString *symbol, NSString *tip, id target, SEL action) {
    NSButton *button = [NSButton buttonWithImage:icon(symbol, 12) target:target action:action];
    button.bordered = NO; button.toolTip = tip; button.accessibilityLabel = tip;
    button.contentTintColor = NSColor.secondaryLabelColor;
    return button;
}
static NSTextField *text(NSString *value, CGFloat size, NSColor *color) {
    NSTextField *field = [NSTextField labelWithString:value];
    field.font = [NSFont systemFontOfSize:size]; field.textColor = color;
    field.lineBreakMode = NSLineBreakByTruncatingTail;
    return field;
}

@interface AxeFlippedView : NSView
@end
@implementation AxeFlippedView
- (BOOL)isFlipped { return YES; }
@end

@interface AxeAudioStrip () <AVAudioPlayerDelegate>
@property(nonatomic, strong) AxeAudioClips *clips;
@property(nonatomic, strong) AxeAudioRecorder *recorder;
@property(nonatomic, strong) NSButton *disclosure, *openButton, *recordButton, *settingsButton;
@property(nonatomic, strong) NSTextField *title, *timeLabel, *message;
@property(nonatomic, strong) NSPopUpButton *devices;
@property(nonatomic, strong) NSLevelIndicator *meter;
@property(nonatomic, strong) NSScrollView *listScroll;
@property(nonatomic, strong) AxeFlippedView *list;
@property(nonatomic, strong) NSBox *rule;
@property(nonatomic, strong) AVAudioPlayer *player;
@property(nonatomic, copy) NSString *playing;
@end

@implementation AxeAudioStrip
- (BOOL)isFlipped { return YES; }
- (instancetype)initWithFrame:(NSRect)frame {
    if (!(self = [super initWithFrame:frame])) return nil;
    _clips = [AxeAudioClips shared];
    _recorder = [AxeAudioRecorder new];
    __weak AxeAudioStrip *weakSelf = self;
    _recorder.changed = ^{ [weakSelf recorderChanged]; };
    _expanded = [NSUserDefaults.standardUserDefaults boolForKey:kExpandedKey];

    _disclosure = iconButton(@"chevron.right", @"Show audio tools", self, @selector(toggle:));
    _title = text(@"Audio", 11, NSColor.secondaryLabelColor);
    _title.font = [NSFont systemFontOfSize:11 weight:NSFontWeightSemibold];
    _openButton = iconButton(@"doc.badge.plus", @"Import a reference audio file", self, @selector(openFile:));
    _devices = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    _devices.controlSize = NSControlSizeSmall; _devices.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
    _devices.target = self; _devices.action = @selector(deviceChosen:);
    _devices.accessibilityLabel = @"Audio input";
    _recordButton = [NSButton buttonWithImage:icon(@"record.circle", 15) target:self action:@selector(record:)];
    _recordButton.bordered = NO; _recordButton.contentTintColor = NSColor.systemRedColor;
    _recordButton.toolTip = @"Record the current tone (up to 2 minutes)"; _recordButton.accessibilityLabel = @"Record";
    _meter = [[NSLevelIndicator alloc] initWithFrame:NSZeroRect];
    _meter.levelIndicatorStyle = NSLevelIndicatorStyleContinuousCapacity;
    _meter.minValue = 0; _meter.maxValue = 60; _meter.warningValue = 54; _meter.criticalValue = 58;
    _timeLabel = text(@"0:00", 11, NSColor.secondaryLabelColor);
    _timeLabel.font = [NSFont monospacedDigitSystemFontOfSize:11 weight:NSFontWeightRegular];
    _message = text(@"", 11, NSColor.secondaryLabelColor);
    _settingsButton = [NSButton buttonWithTitle:@"Open Settings" target:self action:@selector(openPrivacy:)];
    _settingsButton.controlSize = NSControlSizeSmall; _settingsButton.hidden = YES;
    _list = [[AxeFlippedView alloc] initWithFrame:NSZeroRect];
    _listScroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
    _listScroll.documentView = _list; _listScroll.drawsBackground = NO;
    _listScroll.hasVerticalScroller = YES; _listScroll.autohidesScrollers = YES;
    _rule = [[NSBox alloc] initWithFrame:NSZeroRect]; _rule.boxType = NSBoxSeparator;
    for (NSView *view in @[_disclosure, _title, _openButton, _devices, _recordButton, _meter, _timeLabel, _message,
                           _settingsButton, _listScroll, _rule]) [self addSubview:view];
    [self registerForDraggedTypes:@[NSPasteboardTypeFileURL]];
    [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(clipsChanged:) name:AxeAudioClipsChangedNotification object:nil];
    [self reloadDevices];
    [self reloadClips];
    [self applyExpanded];
    return self;
}
- (void)dealloc { [NSNotificationCenter.defaultCenter removeObserver:self]; }

- (CGFloat)preferredHeight {
    if (!self.expanded) return kHeaderRow;
    NSUInteger rows = MIN(self.clips.clips.count, (NSUInteger)kMaxVisibleRows);
    CGFloat listHeight = rows ? rows * kRow + 4 : 0;
    return kHeaderRow + kControlsRow + listHeight + (self.message.stringValue.length ? kMessageRow : 0) + 6;
}
- (void)changedHeight { [self resizeSubviewsWithOldSize:self.bounds.size]; if (self.heightChanged) self.heightChanged(); }
- (void)applyExpanded {
    self.disclosure.image = icon(self.expanded ? @"chevron.down" : @"chevron.right", 11);
    self.disclosure.toolTip = self.expanded ? @"Hide audio tools" : @"Show audio tools";
    for (NSView *view in @[self.devices, self.recordButton, self.meter, self.timeLabel, self.listScroll, self.message])
        view.hidden = !self.expanded;
    self.settingsButton.hidden = !self.expanded || self.settingsButton.tag == 0;
    [self updateTitle];
}
- (void)setExpanded:(BOOL)expanded {
    _expanded = expanded;
    [NSUserDefaults.standardUserDefaults setBool:expanded forKey:kExpandedKey];
    [self applyExpanded]; [self changedHeight];
}
- (void)toggle:(id)sender { self.expanded = !self.expanded; }
- (void)updateTitle {
    NSArray *clips = self.clips.clips;
    NSUInteger refs = [[clips filteredArrayUsingPredicate:[NSPredicate predicateWithFormat:@"role == 'reference'"]] count];
    self.title.stringValue = clips.count ? [NSString stringWithFormat:@"Audio · %lu reference, %lu take%@", (unsigned long)refs,
        (unsigned long)(clips.count - refs), clips.count - refs == 1 ? @"" : @"s"] : @"Audio";
}
- (void)resizeSubviewsWithOldSize:(NSSize)oldSize {
    CGFloat width = self.bounds.size.width;
    self.disclosure.frame = NSMakeRect(kPad - 2, 5, 18, 18);
    self.title.frame = NSMakeRect(kPad + 18, 7, width - 2 * kPad - 46, 16);
    self.openButton.frame = NSMakeRect(width - kPad - 20, 5, 20, 18);
    CGFloat y = kHeaderRow;
    self.recordButton.frame = NSMakeRect(kPad - 2, y + 4, 24, 24);
    self.timeLabel.frame = NSMakeRect(width - kPad - 34, y + 9, 34, 14);
    self.meter.frame = NSMakeRect(NSMinX(self.timeLabel.frame) - 64, y + 11, 58, 10);
    self.devices.frame = NSMakeRect(kPad + 26, y + 5, MAX(60, NSMinX(self.meter.frame) - kPad - 32), 22);
    y += kControlsRow;
    NSArray *clips = self.clips.clips;
    CGFloat visible = MIN(clips.count, (NSUInteger)kMaxVisibleRows) * kRow + (clips.count ? 4 : 0);
    self.listScroll.frame = NSMakeRect(kPad - 4, y, width - 2 * kPad + 8, visible);
    self.list.frame = NSMakeRect(0, 0, NSWidth(self.listScroll.frame) - (clips.count > kMaxVisibleRows ? 14 : 0), clips.count * kRow);
    [self layoutRows];
    y += visible;
    BOOL settings = !self.settingsButton.hidden;
    self.message.frame = NSMakeRect(kPad, y + 2, width - 2 * kPad - (settings ? 96 : 0), 14);
    self.settingsButton.frame = NSMakeRect(width - kPad - 92, y - 2, 92, 20);
    self.rule.frame = NSMakeRect(0, self.bounds.size.height - 1, width, 1);
}

#pragma mark Clip rows
- (void)clipsChanged:(NSNotification *)note { [self reloadClips]; [self changedHeight]; }
- (void)reloadClips {
    for (NSView *row in self.list.subviews.copy) [row removeFromSuperview];
    NSUInteger index = 0;
    for (NSDictionary *clip in self.clips.clips) {
        AxeFlippedView *row = [[AxeFlippedView alloc] initWithFrame:NSZeroRect];
        row.identifier = clip[@"id"];
        NSButton *play = iconButton([clip[@"id"] isEqual:self.playing] ? @"stop.fill" : @"play.fill", @"Play", self, @selector(play:));
        play.identifier = clip[@"id"];
        NSTextField *name = text(clip[@"name"], 12, NSColor.labelColor);
        name.toolTip = [NSString stringWithFormat:@"%@ · %@", clip[@"name"], clip[@"source"]];
        NSSegmentedControl *role = [NSSegmentedControl segmentedControlWithLabels:@[@"Ref", @"Mine"]
            trackingMode:NSSegmentSwitchTrackingSelectOne target:self action:@selector(roleChanged:)];
        role.controlSize = NSControlSizeMini; role.font = [NSFont systemFontOfSize:9];
        role.selectedSegment = [clip[@"role"] isEqual:@"reference"] ? 0 : 1;
        role.identifier = clip[@"id"];
        [role setToolTip:@"Reference: the tone to match" forSegment:0];
        [role setToolTip:@"Mine: a recording of your current tone" forSegment:1];
        double seconds = [clip[@"duration_s"] doubleValue];
        NSTextField *length = text([NSString stringWithFormat:@"%d:%02d", (int)seconds / 60, (int)seconds % 60], 10, NSColor.tertiaryLabelColor);
        length.font = [NSFont monospacedDigitSystemFontOfSize:10 weight:NSFontWeightRegular];
        NSButton *remove = iconButton(@"trash", @"Delete clip", self, @selector(removeClip:));
        remove.identifier = clip[@"id"];
        for (NSView *view in @[play, name, role, length, remove]) [row addSubview:view];
        [self.list addSubview:row];
        ++index;
    }
    [self updateTitle];
}
- (void)layoutRows {
    CGFloat width = NSWidth(self.list.frame);
    NSUInteger index = 0;
    for (NSView *row in self.list.subviews) {
        row.frame = NSMakeRect(0, index++ * kRow, width, kRow);
        NSArray *views = row.subviews;   // play, name, role, length, remove
        [views[0] setFrame:NSMakeRect(4, 4, 18, 18)];
        [views[4] setFrame:NSMakeRect(width - 22, 4, 18, 18)];
        [views[3] setFrame:NSMakeRect(width - 58, 7, 32, 13)];
        [views[2] setFrame:NSMakeRect(width - 140, 4, 78, 18)];
        [views[1] setFrame:NSMakeRect(26, 6, MAX(20, width - 172), 15)];
    }
}
- (void)roleChanged:(NSSegmentedControl *)sender {
    [self.clips setRole:sender.selectedSegment == 0 ? @"reference" : @"current" forClip:sender.identifier];
}
- (void)removeClip:(NSButton *)sender {
    if ([sender.identifier isEqual:self.playing]) [self stopPlayback];
    [self.clips removeClip:sender.identifier];
}
- (void)play:(NSButton *)sender {
    NSString *identifier = sender.identifier;
    BOOL same = [identifier isEqual:self.playing];
    [self stopPlayback];
    if (same) return;
    NSError *error = nil;
    self.player = [[AVAudioPlayer alloc] initWithContentsOfURL:[self.clips fileForClip:identifier] error:&error];
    self.player.delegate = self;
    if (!self.player || ![self.player play]) { [self showMessage:@"This clip cannot be played" settings:NO]; return; }
    self.playing = identifier; [self reloadClips]; [self layoutRows];
}
- (void)stopPlayback {
    [self.player stop]; self.player = nil;
    if (self.playing) { self.playing = nil; [self reloadClips]; [self layoutRows]; }
}
- (void)audioPlayerDidFinishPlaying:(AVAudioPlayer *)player successfully:(BOOL)flag { [self stopPlayback]; }

#pragma mark Import
- (void)openFile:(id)sender {
    NSOpenPanel *panel = [NSOpenPanel openPanel];
    panel.allowedContentTypes = @[UTTypeAudio];
    panel.allowsMultipleSelection = YES;
    panel.message = @"Choose reference audio — ideally isolated guitar playing the part you want to match.";
    [panel beginSheetModalForWindow:self.window completionHandler:^(NSModalResponse response) {
        if (response == NSModalResponseOK) [self importURLs:panel.URLs];
    }];
}
- (void)importURLs:(NSArray<NSURL *> *)urls {
    NSString *last = nil, *failure = nil;
    for (NSURL *url in urls) {
        NSDictionary *clip = [self.clips importFile:url failure:&failure];
        if (clip) last = clip[@"name"];
    }
    if (!self.expanded) self.expanded = YES;
    if (last) [self showMessage:[NSString stringWithFormat:@"Reference “%@” ready — ask the assistant to compare.", last] settings:NO];
    else if (failure) [self showMessage:failure settings:NO];
}
- (NSDragOperation)draggingEntered:(id<NSDraggingInfo>)info {
    return [self audioURLs:info].count ? NSDragOperationCopy : NSDragOperationNone;
}
- (NSArray<NSURL *> *)audioURLs:(id<NSDraggingInfo>)info {
    NSArray *urls = [info.draggingPasteboard readObjectsForClasses:@[NSURL.class] options:@{NSPasteboardURLReadingFileURLsOnlyKey:@YES,
        NSPasteboardURLReadingContentsConformToTypesKey:@[UTTypeAudio.identifier]}];
    return urls ?: @[];
}
- (BOOL)performDragOperation:(id<NSDraggingInfo>)info { [self importURLs:[self audioURLs:info]]; return YES; }

#pragma mark Recording
- (void)reloadDevices {
    [self.devices removeAllItems];
    NSString *saved = [NSUserDefaults.standardUserDefaults stringForKey:kDeviceKey];
    for (NSDictionary *device in [AxeAudioRecorder inputDevices]) {
        [self.devices addItemWithTitle:device[@"name"]];
        self.devices.lastItem.representedObject = device[@"id"];
        // Prefer the remembered input, else an Axe-Fx interface when present.
        if ([device[@"id"] isEqual:saved] || (!saved && [device[@"name"] localizedCaseInsensitiveContainsString:@"Axe-Fx"]))
            [self.devices selectItem:self.devices.lastItem];
    }
    if (!self.devices.numberOfItems) { [self.devices addItemWithTitle:@"No audio input"]; self.devices.enabled = NO; }
}
- (void)deviceChosen:(id)sender {
    [NSUserDefaults.standardUserDefaults setObject:self.devices.selectedItem.representedObject forKey:kDeviceKey];
}
- (void)record:(id)sender {
    if (self.recorder.recording) {
        [self.recorder stop:^(NSDictionary *clip, NSString *failure) {
            [self showMessage:clip ? [NSString stringWithFormat:@"Take “%@” saved — ask the assistant to compare it with the reference.", clip[@"name"]]
                                   : (failure ?: @"Recording failed") settings:NO];
        }];
        return;
    }
    [self stopPlayback];
    [AxeAudioRecorder requestAccess:^(BOOL granted) {
        if (!granted) {
            [self showMessage:@"Microphone access is off for Axe-Edit III Chat." settings:YES];
            return;
        }
        NSString *failure = nil;
        if ([self.recorder startWithDevice:self.devices.selectedItem.representedObject failure:&failure])
            [self showMessage:@"Recording — play the part, then press stop." settings:NO];
        else [self showMessage:failure settings:NO];
    }];
}
- (void)recorderChanged {
    BOOL recording = self.recorder.recording;
    self.recordButton.image = icon(recording ? @"stop.circle.fill" : @"record.circle", 15);
    self.recordButton.toolTip = recording ? @"Stop recording" : @"Record the current tone (up to 2 minutes)";
    self.recordButton.accessibilityLabel = recording ? @"Stop recording" : @"Record";
    self.devices.enabled = !recording && self.devices.numberOfItems > 0;
    self.meter.doubleValue = recording ? self.recorder.level + 60 : 0;
    int seconds = (int)self.recorder.elapsed;
    self.timeLabel.stringValue = [NSString stringWithFormat:@"%d:%02d", seconds / 60, seconds % 60];
    self.timeLabel.textColor = recording ? NSColor.systemRedColor : NSColor.secondaryLabelColor;
}
- (void)showMessage:(NSString *)message settings:(BOOL)settings {
    BOOL hadMessage = self.message.stringValue.length > 0;
    self.message.stringValue = message ?: @"";
    self.message.toolTip = message;
    self.settingsButton.tag = settings; self.settingsButton.hidden = !settings || !self.expanded;
    if (hadMessage != (message.length > 0) || settings) [self changedHeight];
    else [self resizeSubviewsWithOldSize:self.bounds.size];
}
- (void)openPrivacy:(id)sender {
    [NSWorkspace.sharedWorkspace openURL:[NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"]];
}
@end
