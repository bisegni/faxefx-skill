// Read the owned copy's Axe-Edit accessibility tree; write parameters only
// through Axe-Edit's own Undo-aware command path (NativeBridge). Axe-Edit owns
// the MIDI transport; no SysEx, mouse or keyboard synthesis is used.
#import "EditorTools.h"
#import "NativeBridge.h"
#import "AudioClips.h"
#import "AudioTools.h"
#import <objc/message.h>
#include <cmath>

static id attr(id node, NSString *name) {
    NSDictionary *selectors = @{@"role":@"accessibilityRole",@"label":@"accessibilityLabel",
        @"value":@"accessibilityValue",@"children":@"accessibilityChildren",@"min":@"accessibilityMinValue",
        @"max":@"accessibilityMaxValue",@"enabled":@"isAccessibilityEnabled",@"title":@"accessibilityTitle",@"focus":@"accessibilityFocusedUIElement",@"window":@"accessibilityWindow"};
    SEL selector = NSSelectorFromString(selectors[name]);
    @try {
        if (![node respondsToSelector:selector]) return nil;
        if ([name isEqual:@"enabled"]) return @(((BOOL (*)(id, SEL))objc_msgSend)(node, selector));
        return ((id (*)(id, SEL))objc_msgSend)(node, selector);
    } @catch (NSException *exception) { return nil; }
}
static NSString *labelOf(id node) {
    for(NSString *key in @[@"label",@"title"]) {id value=attr(node,key);if([value isKindOfClass:NSString.class] && [value length])return value;}
    return @"";
}
static NSNumber *numberOf(id value) {
    if([value isKindOfClass:NSNumber.class])return value;
    if(![value isKindOfClass:NSString.class])return nil;
    NSScanner *scanner=[NSScanner scannerWithString:value];double number=0;
    return [scanner scanDouble:&number] && scanner.atEnd && std::isfinite(number) ? @(number) : nil;
}
static NSNumber *boolOf(id value) {
    if([value isKindOfClass:NSNumber.class])return @([value boolValue]);
    if([value isKindOfClass:NSString.class]) {NSString *s=[value lowercaseString];if([@[@"on",@"true",@"1"] containsObject:s])return @YES;if([@[@"off",@"false",@"0"] containsObject:s])return @NO;}
    return nil;
}
static NSArray *nodesOf(id root) {
    if (!root) return @[];
    NSMutableArray *out = [NSMutableArray array], *queue = [NSMutableArray arrayWithObject:root];
    NSHashTable *seen = [NSHashTable hashTableWithOptions:NSPointerFunctionsObjectPointerPersonality];
    for (NSUInteger i=0; i<queue.count && out.count<4096; ++i) {
        id node=queue[i]; if ([seen containsObject:node]) continue;
        [seen addObject:node]; [out addObject:node];
        id children=attr(node,@"children");
        if ([children isKindOfClass:NSArray.class] && queue.count+[children count]<8192) [queue addObjectsFromArray:children];
    }
    return out;
}
static BOOL allowed(id node, SEL action) {
    if(![node respondsToSelector:action] || [attr(node,@"enabled") isEqual:@NO])return NO;
    SEL query=@selector(isAccessibilitySelectorAllowed:);
    @try {return ![node respondsToSelector:query] || ((BOOL (*)(id,SEL,SEL))objc_msgSend)(node,query,action);}
    @catch(NSException *exception) {return NO;}
}
static BOOL press(id node) {
    SEL selector=@selector(accessibilityPerformPress);
    @try { return allowed(node,selector) && ((BOOL (*)(id,SEL))objc_msgSend)(node,selector); }
    @catch (NSException *exception) { return NO; }
}
static BOOL setValue(id node,id value) {
    SEL selector=@selector(setAccessibilityValue:);
    @try {
        if (!allowed(node,selector)) return NO;
        // JUCE's native setter takes NSString and calls setValueAsString.
        // Passing NSNumber directly is an ABI-compatible but invalid object.
        NSString *text=[value isKindOfClass:NSString.class] ? value : ([value isKindOfClass:NSNumber.class] ? [value stringValue] : nil);
        if(!text)return NO;
        ((void (*)(id,SEL,id))objc_msgSend)(node,selector,text); return YES;
    } @catch (NSException *exception) { return NO; }
}
// Axe-Edit's custom knobs expose a read-only editable Label rather than a
// slider value interface. Its press action opens JUCE's real TextEditor.
static NSString *textOf(id node) {
    id value=attr(node,@"value");return [value isKindOfClass:NSString.class] ? value : labelOf(node);
}
static id numericLabel(id slider) {
    for(id child in nodesOf(slider)) if(child!=slider && [attr(child,@"role") isEqual:NSAccessibilityTextAreaRole])return child;
    return nil;
}
static NSNumber *displayNumber(id node) {
    NSNumber *value=numberOf(attr(node,@"value"));if(value)return value;
    NSString *text=textOf(numericLabel(node) ?: node);
    NSScanner *scanner=[NSScanner scannerWithString:text];double number=0;
    if(![scanner scanDouble:&number] || !std::isfinite(number))return nil;
    // A numeric display may include units (dB, Hz, %, ms); never parse a
    // parameter title or enum as a numeric value.
    NSString *suffix=[[text substringFromIndex:scanner.scanLocation] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet];
    NSCharacterSet *units=[NSCharacterSet characterSetWithCharactersInString:@"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ%°/ "];
    return [suffix rangeOfCharacterFromSet:units.invertedSet].location==NSNotFound ? @(number) : nil;
}
// Text fields (scene name) are edited only through an accessibility value
// setter the control itself exposes; otherwise the edit is refused.
static BOOL editText(id node,NSString *text,NSString **failure) {
    if(allowed(node,@selector(setAccessibilityValue:)))return setValue(node,text);
    if(failure)*failure=@"This control exposes no native value setter; not edited";
    return NO;
}
// Half a unit of the precision the editor displays (e.g. "2.51" -> 0.005).
static double displayTolerance(NSString *label) {
    NSString *text=[[label componentsSeparatedByString:@","] firstObject];
    NSRange colon=[text rangeOfString:@":"]; if(colon.location!=NSNotFound) text=[text substringFromIndex:colon.location+1];
    NSRange dot=[text rangeOfString:@"."]; NSUInteger digits=0;
    if(dot.location!=NSNotFound) for(NSUInteger i=dot.location+1;i<text.length && [NSCharacterSet.decimalDigitCharacterSet characterIsMember:[text characterAtIndex:i]];++i) ++digits;
    return 0.5*std::pow(10.0,-(double)digits)+1e-9;
}
static NSDictionary *errorResult(NSString *message) { return @{@"ok":@NO,@"error":message}; }
static NSString *parameterName(NSString *label) {
    return [[label componentsSeparatedByString:@":"] firstObject];
}

@interface AxeEditorTools ()
@property(nonatomic, strong) NSMutableDictionary<NSString *,NSDictionary *> *completed;
@property(nonatomic) BOOL executing;
@end

@implementation AxeEditorTools
+ (instancetype)shared {
    static AxeEditorTools *tools; static dispatch_once_t once;
    dispatch_once(&once, ^{ tools=[AxeEditorTools new]; tools.completed=[NSMutableDictionary dictionary]; });
    return tools;
}
+ (NSArray *)specifications {
    NSDictionary *string=@{@"type":@"string"}, *number=@{@"type":@"number"};
    NSDictionary *context=@{@"type":@"string",@"description":@"Exact context token from the latest axefx_inspect result; refresh after each selection or edit."};
    NSDictionary *key=@{@"type":@"string",@"minLength":@1,@"maxLength":@128,@"description":@"Unique operation ID. Retry the same operation with the same key; never replay an uncertain mutation with a new key."};
    NSMutableArray *specs=[NSMutableArray array];
    void (^add)(NSString*,NSString*,NSDictionary*,NSArray*)=^(NSString *name,NSString *description,NSDictionary *properties,NSArray *required) {
        [specs addObject:@{@"name":name,@"description":description,@"inputSchema":@{@"type":@"object",@"properties":properties,@"required":required,@"additionalProperties":@NO}}];
    };
    add(@"axefx_inspect",@"Read the actual connected Axe-Edit UI: current Axe-Fx preset, eight scenes, grid blocks, selected block/channel, parameter values and observed ranges. Never invent missing values.",@{},@[]);
    NSDictionary *base=@{@"expected_context":context,@"operation_id":key};
    NSMutableDictionary *(^props)(NSDictionary*)=^NSMutableDictionary*(NSDictionary *extra) { NSMutableDictionary *p=[base mutableCopy];[p addEntriesFromDictionary:extra];return p; };
    add(@"axefx_select_block",@"Select an existing effect by its inspected grid label. Returns the selected block's actual parameters; does not insert or remove blocks.",props(@{@"block":string}),@[@"block",@"expected_context",@"operation_id"]);
    add(@"axefx_set_parameter",@"Change one numeric knob of the selected effect through Axe-Edit's own Undo-aware parameter command (the same path as typing a value into the knob). Requires the exact current value as expected_value and a value within the inspected minimum..maximum. Only parameters inspected as writable=true can be set; not_writable_reason explains the rest. Returns editor readback; the user can undo with Cmd-Z; hardware persistence is not claimed.",props(@{@"parameter":string,@"value":number,@"expected_value":number}),@[@"parameter",@"value",@"expected_value",@"expected_context",@"operation_id"]);
    add(@"axefx_set_bypass",@"Set the selected effect's bypass state in the current scene; verify the returned UI state.",props(@{@"bypassed":@{@"type":@"boolean"}}),@[@"bypassed",@"expected_context",@"operation_id"]);
    add(@"axefx_select_channel",@"Choose an existing channel A-D for the selected effect in the current scene. Other scenes/channels are not rewritten.",props(@{@"channel":@{@"type":@"string",@"enum":@[@"A",@"B",@"C",@"D"]}}),@[@"channel",@"expected_context",@"operation_id"]);
    add(@"axefx_select_scene",@"Select one of the Axe-Fx's eight existing scene slots (1-8). Build a scene by selecting it, setting block bypass/channels/parameters, and naming it. There are no extra scene slots.",props(@{@"scene":@{@"type":@"integer",@"minimum":@1,@"maximum":@8}}),@[@"scene",@"expected_context",@"operation_id"]);
    add(@"axefx_name_scene",@"Rename the current scene using Axe-Edit's editable scene-name control; changes the edit buffer, never stores the preset.",props(@{@"name":@{@"type":@"string",@"minLength":@1,@"maxLength":@32}}),@[@"name",@"expected_context",@"operation_id"]);
    [specs addObjectsFromArray:[AxeAudioTools specifications]];
    return specs;
}
+ (NSString *)systemPrompt {
    return @"You are an active guitar-tone assistant for the user's Axe-Fx III, embedded in its real Axe-Edit editor. Help create useful guitar tones and scenes from musical descriptions, playing feedback, reference rigs and recordings when supported. Use the provided axefx_* tools to inspect and edit the actual connected Axe-Fx through Axe-Edit. The user's tone-making request authorizes reasonable current-edit-buffer changes; do not merely give manual knob instructions when a supported tool can perform the requested edit. Start with axefx_inspect, select the intended block, and use observed parameter names, values, channels, bypass state and known effect identities. Respect each parameter's inspected minimum/maximum and only set parameters marked writable=true; never invent ranges. Each edit is a normal Axe-Edit edit the user can undo with Cmd-Z. Parameter changes affect the block channel and may also affect other scenes using that same channel; explain this when building scene-specific tones. Work in small musical steps and ask the user to play and report what they hear.\n\nTools: axefx_inspect reads current preset/scenes/effects/selected parameters and context. axefx_select_block selects an existing grid block. axefx_set_parameter changes a writable numeric knob using its expected prior value. axefx_set_bypass and axefx_select_channel configure a block for the current scene. axefx_select_scene selects one of the eight fixed slots; use it with bypass/channel/parameter edits and axefx_name_scene to build scenes. Effect model changes, block insertion/deletion and cable changes are not exposed yet; do not claim to perform them or guess UI coordinates.\n\nEvery selection or mutation needs the latest expected_context; refresh inspection after each edit. Use a unique operation_id per intended mutation and preserve it on retries. Never retry an uncertain write as a new operation. A disconnected device, stale context, unsupported value, pending operation, or Stop must halt edits. Report tool failures and partial results honestly. Readback verification_scope=axe_edit_ui confirms only the editor's value, not an independent hardware getter or persistence. Never claim to have heard audio: you only receive numeric measurements from the audio tools. Preset Save/Store, raw MIDI, firmware, global settings, shell, filesystem, web, connectors and delegated agents are unavailable. Never instruct a workaround to bypass these limits. Do not save a preset automatically. Keep explanations musical, concise, and specific to the user's guitar-tone goal.\n\nTone matching from audio: the user keeps local clips in the panel's Audio section, marked reference (the tone to match, ideally isolated guitar) or current (a recording of their Axe-Fx tone). axefx_audio_list shows them, axefx_audio_analyze measures one, and axefx_audio_compare reports how the current tone differs from the reference. Workflow: 1) axefx_audio_list; if a reference or current clip is missing, ask the user to import or record one. 2) axefx_inspect the relevant blocks (amp, cab, drive, EQ, delay, reverb). 3) axefx_audio_compare. 4) Explain the biggest differences musically and PROPOSE a short list of concrete changes, each with block, parameter, current -> proposed value and the reason (e.g. high rolloff lower than reference -> raise Cab High Cut); only writable parameters within their inspected ranges, at most about 3-5 changes per round. 5) Do not apply anything until the user explicitly agrees; then apply exactly the approved changes with axefx_set_parameter. 6) Ask the user to play the same part the same way, record a new current take, and compare again, reporting whether the spectral distance shrank. Measurements are proxies that depend on playing and recording; say so when the evidence is weak (mixed sources, different parts, short clips).";
}
- (NSArray *)nodes { return nodesOf(self.editor); }
- (id<AxeParameterWriter>)writer { return self.parameterWriter ?: [AxeNativeBridge shared]; }
- (NSDictionary *)diagnostics {
    NSMutableArray *tree=[NSMutableArray array];
    for(id node in [self nodes]) {
        NSMutableDictionary *row=[@{@"class":NSStringFromClass([node class]),@"label":labelOf(node),@"settable":@(allowed(node,@selector(setAccessibilityValue:))),@"pressable":@(allowed(node,@selector(accessibilityPerformPress)))} mutableCopy];
        for(NSString *key in @[@"role",@"value",@"min",@"max",@"enabled"]) {id value=attr(node,key);if([value isKindOfClass:NSString.class] || [value isKindOfClass:NSNumber.class])row[key]=value;}
        [tree addObject:row];
    }
    return @{@"inspection":[self inspect],@"tree":tree};
}
- (id)find:(NSString *)role label:(NSString *)label nodes:(NSArray *)nodes {
    for(id node in nodes) if ((!role || [attr(node,@"role") isEqual:role]) &&
        ([labelOf(node) isEqual:label] || [labelOf(node) hasPrefix:[label stringByAppendingString:@":"]])) return node;
    return nil;
}
- (NSDictionary *)inspect {
    NSArray *nodes=[self nodes];
    if([self.writer respondsToSelector:@selector(calibrateWithAccessibilityNodes:)]) [self.writer calibrateWithAccessibilityNodes:nodes];
    BOOL connected=NO;
    NSMutableArray *blocks=[NSMutableArray array], *parameters=[NSMutableArray array], *scenes=[NSMutableArray array];
    NSMutableDictionary *context=[NSMutableDictionary dictionary];
    for(id node in nodes) {
        NSString *label=labelOf(node), *role=attr(node,@"role"); id value=attr(node,@"value");
        if ([label isEqual:@"Connected!"] || [value isEqual:@"Connected!"]) connected=YES;
        for(NSString *field in @[@"Preset Number",@"Preset Name",@"Scene Number",@"Scene Name"])
            if ([label isEqual:field] || [label hasPrefix:[field stringByAppendingString:@":"]])
                {
                NSString *text=[value isKindOfClass:NSString.class] ? value : label;
                NSString *prefix=[field stringByAppendingString:@":"];
                context[field]=[text hasPrefix:prefix] ? [[text substringFromIndex:prefix.length] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet] : text;
            }
        if ([label hasPrefix:@"Grid row "] && ![label hasSuffix:@": Empty"]) {
            BOOL selected=[boolOf(value) boolValue];
            [blocks addObject:@{@"block":label,@"selected":@(selected)}];
            if(selected) {context[@"block"]=label;context[@"block_identity"]=[[label componentsSeparatedByString:@":"] firstObject];}
        }
        if ([label hasPrefix:@"Scene "] && [role isEqual:NSAccessibilityButtonRole] &&
            label.length>6 && [NSCharacterSet.decimalDigitCharacterSet characterIsMember:[label characterAtIndex:6]])
            [scenes addObject:@{@"label":label,@"selected":@([boolOf(value) boolValue])}];
        if([label hasPrefix:@"Channel "] && [boolOf(value) boolValue]) context[@"channel"]=label;
        if([role isEqual:NSAccessibilitySliderRole]) {
            value=displayNumber(node); NSString *why=nil;
            NSDictionary *native=[self.writer parameterForAccessibilityNode:node failure:&why];
            NSMutableDictionary *row=[@{@"parameter":parameterName(label),@"label":label,@"value":value ?: NSNull.null,
                @"writable":@(value && native!=nil)} mutableCopy];
            if(native) {
                row[@"minimum"]=native[@"display_min"];row[@"maximum"]=native[@"display_max"];row[@"step"]=@([native[@"step"] doubleValue]*[native[@"scale"] doubleValue]);
                row[@"effect_id"]=native[@"effect"];row[@"parameter_id"]=native[@"parameter"];
            } else row[@"not_writable_reason"]=why ?: @"Unknown";
            [parameters addObject:row];
        }
        if([label hasPrefix:@"Bypass:"] || [label isEqual:@"Bypass"]) context[@"bypassed"]=boolOf(value) ?: NSNull.null;
    }
    NSData *encoded=[NSJSONSerialization dataWithJSONObject:context options:NSJSONWritingSortedKeys error:nil];
    NSString *token=[[NSString alloc] initWithData:encoded encoding:NSUTF8StringEncoding] ?: @"{}";
    connected=connected && context[@"Preset Number"] && context[@"Scene Number"];
    return @{@"ok":@YES,@"connected":@(connected),@"context":context,@"context_token":token,
        @"blocks":blocks,@"scenes":scenes,@"parameters":parameters,
        @"verification_scope":@"axe_edit_ui",@"hardware_persistence_verified":@NO,
        @"capabilities":@{@"parameter_edit":@(connected),@"effect_bypass_and_channels":@(connected),@"scene_configuration":@(connected),
            @"effect_model_selection":@NO,@"block_insertion":@NO,@"block_deletion":@NO,@"routing_edits":@NO,@"preset_store":@NO}};
}
- (void)call:(NSString *)name arguments:(NSDictionary *)args authority:(AxeToolAuthority)authority reply:(AxeToolReply)reply {
    NSAssert(NSThread.isMainThread,@"Editor access must run on the main thread");
    if(!authority()) { reply(errorResult(@"Turn stopped or authority revoked")); return; }
    NSDictionary *definition=nil;for(NSDictionary *spec in [AxeEditorTools specifications])if([spec[@"name"] isEqual:name])definition=spec;
    if(!definition || ![args isKindOfClass:NSDictionary.class]) {reply(errorResult(@"Unknown tool or invalid arguments"));return;}
    NSDictionary *properties=definition[@"inputSchema"][@"properties"];
    for(NSString *field in args)if(!properties[field]) {reply(errorResult(@"Unexpected tool argument"));return;}
    for(NSString *field in definition[@"inputSchema"][@"required"])if(!args[field] || args[field]==NSNull.null) {reply(errorResult(@"A required tool argument is missing"));return;}
    if([name isEqual:@"axefx_inspect"]) { reply([self inspect]); return; }
    // Audio tools are read-only analysis of local clips: no editor context needed.
    if([name hasPrefix:@"axefx_audio_"]) { [AxeAudioTools call:name arguments:args clips:self.audioClips ?: [AxeAudioClips shared] reply:reply]; return; }
    if(self.executing) { reply(errorResult(@"Another editor operation is pending; inspect and retry later")); return; }
    NSString *key=args[@"operation_id"];
    if([key isKindOfClass:NSString.class] && self.completed[key]) {
        NSDictionary *prior=self.completed[key];
        if([prior[@"name"] isEqual:name] && [prior[@"arguments"] isEqual:args]) reply(prior[@"result"]);
        else reply(errorResult(@"operation_id was already used for a different operation"));
        return;
    }
    if(self.completed.count>=512) {reply(errorResult(@"Operation ledger is full; restart the app before further writes"));return;}
    NSDictionary *before=[self inspect];
    if(![before[@"connected"] boolValue]) { reply(errorResult(@"Axe-Edit does not report a connected Axe-Fx")); return; }
    if(![args[@"expected_context"] isEqual:before[@"context_token"]]) { reply(errorResult(@"Stale editor context; call axefx_inspect again")); return; }
    if(![key isKindOfClass:NSString.class] || !key.length || key.length>256) { reply(errorResult(@"operation_id is required")); return; }
    NSArray *nodes=[self nodes]; id target=nil; BOOL accepted=NO;
    NSString *verifyKind=nil; id wanted=nil; NSString *failure=nil; NSNumber *restore=nil;
    self.executing=YES;
    if([name isEqual:@"axefx_select_block"]) {
        NSString *block=args[@"block"];
        if([block isKindOfClass:NSString.class] && [block hasPrefix:@"Grid row "]) target=[self find:nil label:block nodes:nodes];
        accepted=press(target);verifyKind=@"selected_block";wanted=block;
    } else if([name isEqual:@"axefx_set_parameter"]) {
        NSString *parameter=args[@"parameter"]; NSNumber *number=args[@"value"], *expected=args[@"expected_value"];
        for(id node in nodes) if([attr(node,@"role") isEqual:NSAccessibilitySliderRole] && [parameterName(labelOf(node)) isEqual:parameter]) {
            if(target) { failure=@"Parameter label is ambiguous"; break; } target=node;
        }
        NSNumber *current=displayNumber(target);
        if(!failure && (![number isKindOfClass:NSNumber.class] || CFGetTypeID((__bridge CFTypeRef)number)==CFBooleanGetTypeID() || !std::isfinite(number.doubleValue) || ![expected isKindOfClass:NSNumber.class] || CFGetTypeID((__bridge CFTypeRef)expected)==CFBooleanGetTypeID() || !std::isfinite(expected.doubleValue) ||
            !current || ![current isEqual:expected])) failure=@"Value or expected_value is invalid/stale";
        // Native path only: the knob's own effect/parameter IDs, the firmware
        // definition and the editor's Undo-aware command. Range is enforced there.
        if(!failure && target) accepted=[self.writer setDisplayValue:number.doubleValue forAccessibilityNode:target failure:&failure];
        if(!target && !failure) failure=@"Parameter not found on the selected block";
        restore=current;
        verifyKind=@"parameter";wanted=@{@"parameter":parameter ?: @"",@"value":number ?: NSNull.null};
    } else if([name isEqual:@"axefx_set_bypass"] || [name isEqual:@"axefx_select_channel"] || [name isEqual:@"axefx_select_scene"]) {
        NSString *label=nil;
        if([name isEqual:@"axefx_set_bypass"] && [args[@"bypassed"] isKindOfClass:NSNumber.class] && CFGetTypeID((__bridge CFTypeRef)args[@"bypassed"])==CFBooleanGetTypeID()) { label=@"Bypass";wanted=args[@"bypassed"]; }
        if([name isEqual:@"axefx_select_channel"] && [@[@"A",@"B",@"C",@"D"] containsObject:args[@"channel"]]) { label=[@"Channel " stringByAppendingString:args[@"channel"]];wanted=@YES; }
        if([name isEqual:@"axefx_select_scene"] && [args[@"scene"] isKindOfClass:NSNumber.class] && CFGetTypeID((__bridge CFTypeRef)args[@"scene"])!=CFBooleanGetTypeID() && [args[@"scene"] doubleValue]==[args[@"scene"] integerValue] && [args[@"scene"] integerValue]>=1 && [args[@"scene"] integerValue]<=8) {
            label=[NSString stringWithFormat:@"Scene %ld",[args[@"scene"] integerValue]];wanted=@YES;
        }
        if(label) target=[self find:NSAccessibilityButtonRole label:label nodes:nodes];
        NSNumber *current=boolOf(attr(target,@"value"));
        accepted=target && current && ([current boolValue]==[wanted boolValue] || press(target));verifyKind=@"button";
        wanted=@{@"label":label ?: @"",@"value":wanted ?: NSNull.null};
    } else if([name isEqual:@"axefx_name_scene"]) {
        NSString *value=args[@"name"];
        target=[self find:nil label:@"Scene Name" nodes:nodes];
        if([value isKindOfClass:NSString.class] && value.length && value.length<=32 && [value rangeOfCharacterFromSet:NSCharacterSet.controlCharacterSet].location==NSNotFound) accepted=editText(target,value,&failure);
        verifyKind=@"scene_name";wanted=value;
    } else { self.executing=NO;reply(errorResult(@"Unknown Axe-Fx tool"));return; }
    if(!accepted) { self.executing=NO;reply(errorResult(failure ?: @"Native editor control is unavailable or refused the operation"));return; }
    self.executing=YES;
    // Cache accepted operations immediately. Stop, context changes or a timeout
    // must not turn a retry into a second write.
    NSDictionary *pending=@{@"ok":@NO,@"status":@"sent_readback_pending",@"retry_write":@NO};
    self.completed[key]=@{@"name":name,@"arguments":args,@"result":pending};
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 500*NSEC_PER_MSEC),dispatch_get_main_queue(), ^{
        self.executing=NO;
        NSDictionary *after=[self inspect]; BOOL verified=NO;
        if([verifyKind isEqual:@"selected_block"]) verified=[after[@"context"][@"block"] isEqual:wanted];
        if([verifyKind isEqual:@"parameter"]) for(NSDictionary *parameter in after[@"parameters"])
            if([parameter[@"parameter"] isEqual:wanted[@"parameter"]] && [parameter[@"value"] isKindOfClass:NSNumber.class])
                verified=std::abs([parameter[@"value"] doubleValue]-[wanted[@"value"] doubleValue])<=displayTolerance(parameter[@"label"]);
        if([verifyKind isEqual:@"button"]) {
            id fresh=[self find:NSAccessibilityButtonRole label:wanted[@"label"] nodes:[self nodes]];
            NSNumber *current=boolOf(attr(fresh,@"value"));
            verified=fresh && current && current.boolValue==[wanted[@"value"] boolValue];
        }
        if([verifyKind isEqual:@"scene_name"]) verified=[after[@"context"][@"Scene Name"] isEqual:wanted];
        NSMutableArray *stableFields=[NSMutableArray arrayWithArray:@[@"Preset Number",@"Preset Name"]];
        if(![name isEqual:@"axefx_select_scene"]) [stableFields addObject:@"Scene Number"];
        if([name isEqual:@"axefx_set_parameter"] || [name isEqual:@"axefx_set_bypass"] || [name isEqual:@"axefx_name_scene"]) [stableFields addObjectsFromArray:@[@"block_identity",@"channel"]];
        for(NSString *field in stableFields) if(![(before[@"context"][field] ?: NSNull.null) isEqual:(after[@"context"][field] ?: NSNull.null)]) verified=NO;
        if(![after[@"connected"] boolValue])verified=NO;
        NSString *status=!authority() ? @"stopped_after_send" : (verified ? @"ui_readback_verified" : @"sent_readback_unverified");
        // A native write whose editor readback disagrees is put back to the
        // value shown before the write (same verified conversion), then reported.
        if(!verified && restore && [verifyKind isEqual:@"parameter"]) {
            id node=nil; for(id candidate in [self nodes]) if([attr(candidate,@"role") isEqual:NSAccessibilitySliderRole] && [parameterName(labelOf(candidate)) isEqual:wanted[@"parameter"]]) node=candidate;
            NSString *restoreFailure=nil;
            status=node && [self.writer setDisplayValue:restore.doubleValue forAccessibilityNode:node failure:&restoreFailure] ? @"readback_mismatch_restore_sent" : @"readback_mismatch_restore_failed_use_undo";
        }
        NSDictionary *result=@{@"ok":@(verified && authority()),@"status":status,
            @"retry_write":@NO,@"before":before,@"after":after,@"verification_scope":@"axe_edit_ui",@"hardware_persistence_verified":@NO};
        self.completed[key]=@{@"name":name,@"arguments":args,@"result":result};
        reply(result);
    });
}
@end
