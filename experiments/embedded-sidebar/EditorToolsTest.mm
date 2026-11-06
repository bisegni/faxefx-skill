// Authored accessibility fixtures: they prove tool semantics, not hardware.
#import "EditorTools.h"
#import "NativeBridge.h"
#include <assert.h>
#include <cmath>

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
- (id)accessibilityFocusedUIElement{return self.focused;}
- (NSWindow *)accessibilityWindow{return self.accessWindow;}
- (BOOL)isAccessibilitySelectorAllowed:(SEL)selector{return selector==@selector(setAccessibilityValue:) ? self.writable : (selector==@selector(accessibilityPerformPress) ? self.action!=nil : YES);}
- (void)setAccessibilityValue:(id)value{assert([value isKindOfClass:NSString.class]);self.writes++;self.value=value;}
- (BOOL)accessibilityPerformPress{if(self.action)self.action();return self.action!=nil;}
@end

// Stands in for AxeNativeBridge: knobs it knows are writable within 0..10;
// a write updates the fixture's displayed value (or is lost, to test restore).
@interface FakeWriter : NSObject <AxeParameterWriter>
@property(nonatomic,strong) NSHashTable *knobs;
@property(nonatomic) NSUInteger writes;
@property(nonatomic) BOOL dropNextWrite;
@end
@implementation FakeWriter
- (NSDictionary *)parameterForAccessibilityNode:(id)node failure:(NSString **)failure {
    if(![self.knobs containsObject:node]) {if(failure)*failure=@"Control is not an Axe-Edit rotary knob";return nil;}
    return @{@"effect":@58,@"parameter":@1,@"display_min":@0,@"display_max":@10,@"scale":@10,@"step":@0.001};
}
- (BOOL)setDisplayValue:(double)value forAccessibilityNode:(id)node failure:(NSString **)failure {
    if(![self parameterForAccessibilityNode:node failure:failure])return NO;
    if(value<0 || value>10) {if(failure)*failure=@"Value is outside the parameter range";return NO;}
    self.writes++;
    if(self.dropNextWrite) {self.dropNextWrite=NO;return YES;}
    ((TestAX *)node).value=[NSString stringWithFormat:@"%.2f",value];
    return YES;
}
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
    TestAX *gain=node(NSAccessibilitySliderRole,@"Gain: 2.51, Modifier Disabled",@"2.51");
    TestAX *bass=node(NSAccessibilitySliderRole,@"Bass",@"3.32");
    TestAX *bypass=node(NSAccessibilityButtonRole,@"Bypass",@NO);
    __weak TestAX *weakBypass=bypass;bypass.action=^{weakBypass.value=@(![weakBypass.value boolValue]);};
    root.children=@[connection,preset,scene,sceneName,block,gain,bass,bypass];
    // Without a verified Axe-Edit executable the real bridge refuses every knob.
    NSDictionary *snapshot=[tools inspect];assert([snapshot[@"connected"] boolValue]);assert([snapshot[@"parameters"] count]==2);
    assert(![snapshot[@"parameters"][0][@"writable"] boolValue] && [snapshot[@"parameters"][0][@"not_writable_reason"] length]);
    assert(![AxeNativeBridge shared].available);
    NSString *refused=nil;assert(![[AxeNativeBridge shared] setDisplayValue:2.61 forAccessibilityNode:gain failure:&refused] && refused.length);
    // Conversion: knob range must equal cache range x scale; raw = display/scale.
    float raw=0;NSString *why=nil;
    assert(AxeNativeDisplayToRaw(2.61,0,1,10,0,10,&raw,&why) && std::fabs(raw-0.261f)<1e-6);
    assert(AxeNativeDisplayToRaw(-100,-1,1,100,-100,100,&raw,&why) && raw==-1.0f);
    assert(!AxeNativeDisplayToRaw(10.5,0,1,10,0,10,&raw,&why));
    assert(!AxeNativeDisplayToRaw(5,0.05,50,10,0,10,&raw,&why) && [why containsString:@"not verified"]);
    assert(!AxeNativeDisplayToRaw(NAN,0,1,10,0,10,&raw,&why));
    FakeWriter *writer=[FakeWriter new];writer.knobs=[NSHashTable weakObjectsHashTable];[writer.knobs addObject:gain];
    tools.parameterWriter=writer;
    snapshot=[tools inspect];
    NSDictionary *gainRow=snapshot[@"parameters"][0];
    assert([gainRow[@"writable"] boolValue] && [gainRow[@"minimum"] isEqual:@0] && [gainRow[@"maximum"] isEqual:@10] && [gainRow[@"parameter_id"] isEqual:@1]);
    assert(![snapshot[@"parameters"][1][@"writable"] boolValue]);
    NSString *context=snapshot[@"context_token"];
    NSDictionary *args=@{@"parameter":@"Gain",@"value":@2.61,@"expected_value":@2.51,@"expected_context":context,@"operation_id":@"gain-1"};
    NSDictionary *result=call(tools,@"axefx_set_parameter",args,^BOOL{return NO;},nil);assert(![result[@"ok"] boolValue] && writer.writes==0);
    NSMutableDictionary *bad=[args mutableCopy];bad[@"expected_context"]=@"stale";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && writer.writes==0);
    bad=[args mutableCopy];bad[@"value"]=@999;
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && writer.writes==0);
    bad=[args mutableCopy];bad[@"expected_value"]=@2.4;
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && writer.writes==0);
    result=call(tools,@"axefx_set_parameter",args,^BOOL{return YES;},nil);
    assert([result[@"ok"] boolValue] && [result[@"status"] isEqual:@"ui_readback_verified"] && writer.writes==1 && [gain.value isEqual:@"2.61"]);
    result=call(tools,@"axefx_set_parameter",args,^BOOL{return YES;},nil);assert([result[@"ok"] boolValue] && writer.writes==1);
    bad=[args mutableCopy];bad[@"value"]=@2.71;
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && writer.writes==1);
    NSDictionary *bassArgs=@{@"parameter":@"Bass",@"value":@3.4,@"expected_value":@3.32,@"expected_context":context,@"operation_id":@"bass-1"};
    result=call(tools,@"axefx_set_parameter",bassArgs,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && writer.writes==1 && [bass.value isEqual:@"3.32"]);
    // Readback mismatch: the prior displayed value is sent back.
    context=[tools inspect][@"context_token"];writer.dropNextWrite=YES;
    NSDictionary *lost=@{@"parameter":@"Gain",@"value":@2.71,@"expected_value":@2.61,@"expected_context":context,@"operation_id":@"lost-write"};
    result=call(tools,@"axefx_set_parameter",lost,^BOOL{return YES;},nil);
    assert(![result[@"ok"] boolValue] && [result[@"status"] isEqual:@"readback_mismatch_restore_sent"] && writer.writes==3 && [gain.value isEqual:@"2.61"]);
    __block BOOL alive=YES;bad=[args mutableCopy];bad[@"expected_context"]=[tools inspect][@"context_token"];bad[@"value"]=@2.71;bad[@"expected_value"]=@2.61;bad[@"operation_id"]=@"stop-after-send";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return alive;},^{alive=NO;});assert(![result[@"ok"] boolValue] && [result[@"status"] isEqual:@"stopped_after_send"]);
    bad=[args mutableCopy];bad[@"expected_context"]=[tools inspect][@"context_token"];bad[@"value"]=@2.81;bad[@"expected_value"]=@2.71;bad[@"operation_id"]=@"context-after-send";
    result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},^{scene.value=@"S02";});assert(![result[@"ok"] boolValue]);
    scene.value=@"S01";
    // Text controls without a native value setter are refused, not typed into.
    sceneName.writable=NO;
    result=call(tools,@"axefx_name_scene",@{@"name":@"Lead",@"expected_context":[tools inspect][@"context_token"],@"operation_id":@"scene-name-refused"},^BOOL{return YES;},nil);
    assert(![result[@"ok"] boolValue] && [sceneName.value isEqual:@"Clean"]);
    sceneName.writable=YES;
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
    NSUInteger writes=writer.writes;result=call(tools,@"axefx_set_parameter",bad,^BOOL{return YES;},nil);assert(![result[@"ok"] boolValue] && writer.writes==writes);
    puts("PASS: native-bridge fail-closed outside Axe-Edit, display/raw conversion guard, writer-gated writability and ranges, expected value, readback precision, mismatch restore, scene naming without typing, stale context, Stop revocation, disconnect and idempotent retries (authored fixtures only)");
}return 0;}
