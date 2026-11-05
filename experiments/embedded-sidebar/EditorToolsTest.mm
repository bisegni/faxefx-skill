// Authored accessibility fixtures: they prove bridge semantics, not hardware.
#import "EditorTools.h"
#include <assert.h>

@interface TestAX : NSView
@property(nonatomic,copy) NSString *role;
@property(nonatomic,copy) NSString *title;
@property(nonatomic,strong) id value;
@property(nonatomic,strong) id minimum;
@property(nonatomic,strong) id maximum;
@property(nonatomic,strong) NSArray *children;
@property(nonatomic) BOOL writable;
@property(nonatomic,weak) id focused;
@property(nonatomic,weak) NSWindow *accessWindow;
@property(nonatomic) NSUInteger writes;
@property(nonatomic,copy) NSString *inputText;
@property(nonatomic) NSRect inputRect;
@property(nonatomic,copy) void (^inputCommit)(void);
@property(nonatomic,copy) void (^action)(void);
@end
@implementation TestAX
- (NSString *)accessibilityRole{return self.role;}
- (NSString *)accessibilityLabel{return @"";}
- (NSString *)accessibilityTitle{return self.title ?: @"";}
- (id)accessibilityValue{return self.value;}
- (NSArray *)accessibilityChildren{return self.children ?: @[];}
- (id)accessibilityMinValue{return self.minimum;}
- (id)accessibilityMaxValue{return self.maximum;}
- (BOOL)isAccessibilityEnabled{return YES;}
- (NSRect)accessibilityFrame{return self.inputRect;}
- (NSAttributedString *)attributedSubstringForProposedRange:(NSRange)range actualRange:(NSRangePointer)actual {if(actual)*actual=NSMakeRange(0,self.inputText.length);return self.inputText ? [[NSAttributedString alloc] initWithString:self.inputText] : nil;}
- (NSRect)firstRectForCharacterRange:(NSRange)range actualRange:(NSRangePointer)actual{return self.inputRect;}
- (void)insertText:(id)text replacementRange:(NSRange)range {assert(range.location==0 && range.length==self.inputText.length);self.inputText=text;self.writes++;}
- (void)keyDown:(NSEvent *)event {assert(event.keyCode==36);if(self.inputCommit)self.inputCommit();}
- (id)accessibilityFocusedUIElement{return self.focused;}
- (NSWindow *)accessibilityWindow{return self.accessWindow;}
- (BOOL)isAccessibilitySelectorAllowed:(SEL)selector{return selector==@selector(setAccessibilityValue:) ? self.writable : (selector==@selector(accessibilityPerformPress) ? self.action!=nil : YES);}
- (void)setAccessibilityValue:(id)value{assert([value isKindOfClass:NSString.class]);self.writes++;self.value=value;}
- (BOOL)accessibilityPerformPress{if(self.action)self.action();return self.action!=nil;}
@end

@interface TestWindow : NSWindow
@property(nonatomic,copy) void (^commit)(void);
@end
@implementation TestWindow
- (void)sendEvent:(NSEvent *)event {assert(event.keyCode==36);if(self.commit)self.commit();}
@end

static TestAX *node(NSString *role,NSString *title,id value){TestAX *n=[TestAX new];n.role=role;n.title=title;n.value=value;return n;}
static NSDictionary *call(AxeEditorTools *tools,NSString *name,NSDictionary *args,BOOL (^alive)(void),void (^afterSend)(void)) {
    __block NSDictionary *result=nil;
    [tools call:name arguments:args authority:alive reply:^(NSDictionary *value){result=value;}];
    if(afterSend)afterSend();
    NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:2];
    while(!result && deadline.timeIntervalSinceNow>0)[NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
    assert(result);return result;
}

int main(void){@autoreleasepool{
    [NSApplication sharedApplication];
    AxeEditorTools *tools=[AxeEditorTools shared];TestAX *root=[TestAX new];root.role=NSAccessibilityGroupRole;tools.editor=root;
    TestAX *connection=node(NSAccessibilityStaticTextRole,@"",@"Connected!");
    TestAX *preset=node(NSAccessibilityTextAreaRole,@"Preset Number",@"442");
    TestAX *scene=node(NSAccessibilityTextAreaRole,@"Scene Number",@"S01");
    TestAX *sceneName=node(NSAccessibilityTextAreaRole,@"Scene Name",@"Clean");sceneName.writable=YES;
    TestAX *block=node(NSAccessibilityRadioButtonRole,@"Grid row 2 column 3: Amp 1A, Active",@YES);
    TestAX *gain=node(NSAccessibilitySliderRole,@"Gain",@"2.51");gain.minimum=@0;gain.maximum=@10;gain.writable=YES;
    TestAX *bass=node(NSAccessibilitySliderRole,@"Bass",@"3.32");bass.writable=YES;
    TestAX *bypass=node(NSAccessibilityButtonRole,@"Bypass",@NO);
    __weak TestAX *weakBypass=bypass;bypass.action=^{weakBypass.value=@(![weakBypass.value boolValue]);};
    root.children=@[connection,preset,scene,sceneName,block,gain,bass,bypass];
    NSDictionary *snapshot=[tools inspect];assert([snapshot[@"connected"] boolValue]);assert([snapshot[@"parameters"] count]==2);
    assert([snapshot[@"parameters"][1][@"range_status"] isEqual:@"not_exposed_native_control_validates"]);
    NSString *context=snapshot[@"context_token"];
    NSDictionary *args=@{@"parameter":@"Gain",@"value":@2.61,@"expected_value":@2.51,@"expected_context":context,@"operation_id":@"gain-1"};
    NSDictionary *result=call(tools,@"axefx_set_parameter",args,^BOOL{return NO;},nil);assert(![result[@"ok"] boolValue] && gain.writes==0);
    NSMutableDictionary *bad=[args mutableCopy];bad[@"expected_context"]=@"stale";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && gain.writes==0);
    bad=[args mutableCopy];bad[@"value"]=@999;
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && gain.writes==0);
    result=call(tools,@"axefx_set_parameter",args,^BOOL{return YES;},nil);assert([result[@"ok"] boolValue] && gain.writes==1);
    result=call(tools,@"axefx_set_parameter",args,^BOOL{return YES;},nil);assert([result[@"ok"] boolValue] && gain.writes==1);
    bad=[args mutableCopy];bad[@"value"]=@2.71;
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && gain.writes==1);
    NSDictionary *unknown=@{@"parameter":@"Bass",@"value":@3.4,@"expected_value":@3.32,@"expected_context":context,@"operation_id":@"bass-1"};
    result=call(tools,@"axefx_set_parameter",unknown,^BOOL{return YES;},nil);assert([result[@"ok"] boolValue] && bass.writes==1);
    __block BOOL alive=YES;bad=[args mutableCopy];bad[@"value"]=@2.71;bad[@"expected_value"]=@2.61;bad[@"operation_id"]=@"stop-after-send";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return alive;},^{alive=NO;});assert(![result[@"ok"] boolValue] && [result[@"status"] isEqual:@"stopped_after_send"]);
    bad=[args mutableCopy];bad[@"value"]=@2.81;bad[@"expected_value"]=@2.71;bad[@"operation_id"]=@"context-after-send";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},^{scene.value=@"S02";});assert(![result[@"ok"] boolValue]);
    // The real Axe-Edit knob's Label is read-only until its press action opens
    // a focused TextEditor. Only that new editor receives text and Return.
    scene.value=@"S01";
    TestWindow *window=[[TestWindow alloc] initWithContentRect:NSMakeRect(0,0,400,300) styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
    [window.contentView addSubview:root];
    TestAX *display=node(NSAccessibilityTextAreaRole,@"2.81",nil);
    TestAX *entry=node(NSAccessibilityTextAreaRole,@"",@"2.81");entry.writable=YES;entry.accessWindow=window;
    gain.value=nil;gain.children=@[display];gain.writable=NO;
    __weak TestAX *weakDisplay=display;display.action=^{weakDisplay.focused=entry;};
    window.commit=^{weakDisplay.title=entry.value;};
    snapshot=[tools inspect];assert([snapshot[@"parameters"][0][@"value"] isEqual:@2.81]);assert([snapshot[@"parameters"][0][@"writable"] boolValue]);
    bad=[args mutableCopy];bad[@"expected_context"]=snapshot[@"context_token"];bad[@"expected_value"]=@2.81;bad[@"value"]=@2.91;bad[@"operation_id"]=@"numeric-label";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert([result[@"ok"] boolValue] && entry.writes==1 && [display.title isEqual:@"2.91"]);
    // A foreign focus must receive neither text nor a commit key.
    entry.accessWindow=nil;entry.value=@"2.91";bad[@"expected_value"]=@2.91;bad[@"value"]=@3.01;bad[@"operation_id"]=@"foreign-focus";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && entry.writes==1);
    display.action=^{weakDisplay.focused=nil;};display.inputRect=NSMakeRect(10,10,70,20);
    root.inputRect=display.inputRect;root.inputText=@"2.91";
    __weak TestAX *weakRoot=root;root.inputCommit=^{weakDisplay.title=weakRoot.inputText;};
    bad[@"operation_id"]=@"text-input-client";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert([result[@"ok"] boolValue] && root.writes==1 && [display.title isEqual:@"3.01"]);
    root.inputRect=NSMakeRect(300,200,70,20);bad[@"expected_value"]=@3.01;bad[@"value"]=@3.11;bad[@"operation_id"]=@"foreign-input-rectangle";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && root.writes==1);
    NSDictionary *rename=@{@"name":@"Lead",@"expected_context":[tools inspect][@"context_token"],@"operation_id":@"scene-name"};
    result=call(tools,@"axefx_name_scene",rename,^BOOL{return YES;},nil);assert([result[@"ok"] boolValue] && [sceneName.value isEqual:@"Lead"]);
    NSDictionary *fraction=@{@"scene":@1.5,@"expected_context":[tools inspect][@"context_token"],@"operation_id":@"fractional-scene"};
    result=call(tools,@"axefx_select_scene",fraction,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue]);
    // Bypass readback must compare grid identity, not its changing state suffix.
    bypass.action=^{weakBypass.value=@(![weakBypass.value boolValue]);block.title=@"Grid row 2 column 3: Amp 1A, Bypassed";};
    result=call(tools,@"axefx_set_bypass",@{@"bypassed":@YES,@"expected_context":[tools inspect][@"context_token"],@"operation_id":@"bypass-on"},^BOOL{return YES;},nil);assert([result[@"ok"] boolValue]);
    TestAX *a=node(NSAccessibilityButtonRole,@"Channel A",@YES),*b=node(NSAccessibilityButtonRole,@"Channel B",@NO);
    __weak TestAX *weakA=a,*weakB=b;b.action=^{weakA.value=@NO;weakB.value=@YES;block.title=@"Grid row 2 column 3: Amp 1B, Bypassed";};
    root.children=[root.children arrayByAddingObjectsFromArray:@[a,b]];
    result=call(tools,@"axefx_select_channel",@{@"channel":@"B",@"expected_context":[tools inspect][@"context_token"],@"operation_id":@"channel-b"},^BOOL{return YES;},nil);assert([result[@"ok"] boolValue]);
    TestAX *scene2=node(NSAccessibilityButtonRole,@"Scene 2: Off",@NO);__weak TestAX *weakScene2=scene2;
    scene2.action=^{weakScene2.value=@YES;scene.value=@"S02";};root.children=[root.children arrayByAddingObject:scene2];
    result=call(tools,@"axefx_select_scene",@{@"scene":@2,@"expected_context":[tools inspect][@"context_token"],@"operation_id":@"scene-2"},^BOOL{return YES;},nil);assert([result[@"ok"] boolValue]);
    connection.value=@"Disconnected!";bad[@"operation_id"]=@"disconnected";
    NSUInteger writes=gain.writes;result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && gain.writes==writes);
    puts("PASS: native string setters, label fallback, numeric readback, unknown bounds, numeric-entry focus/commit, scene naming, stale/foreign context, Stop revocation, disconnect and idempotent retries (authored fixtures only)");
}return 0;}
