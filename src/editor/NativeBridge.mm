// See NativeBridge.h. All addresses are unslid ARM64 VAs for Axe-Edit III
// 1.15.03 (universal SHA-256 647ba24f...8435); never reuse for other builds.
//
// Verified chain (2026-11-05, live, read-only probes + one authorized
// write/restore of Amp 1 Gain 2.51 -> 2.61 -> 2.51):
//   AX slider (JUCEAccessibilityElement) .handler -> juce::AccessibilityHandler
//   handler+0x08 -> A3RotaryKnob: +0x210 u16 effect ID, +0x212 u16 parameter ID,
//                   +0x23c/+0x240 float display min/max, +0x30 parent component
//   parent = A3EffectEditorContent (primary vtable 0x1011682d8)
//   +0x118 = its A3ComponentComm base (vtable 0x1011685c0); slot +0xa0 is the
//            shared command method 0x1000c8d9c(this, record, flag, undo)
//   comm+0x300 = the editor's Undo context (seen on every user edit)
//   A typed entry calls it with {effect, parameter, op 0x09, raw float}, flag 0.
#import "NativeBridge.h"
#import <AppKit/AppKit.h>
#import <CommonCrypto/CommonDigest.h>
#include <mach-o/dyld.h>
#include <mach-o/getsect.h>
#include <mach/mach.h>
#include <mach/mach_vm.h>
#include <objc/runtime.h>
#include <cmath>
#include <cstring>

static const char *kTextSHA256 = "e7aa716d8c58501a43408ab2643d1f51a3b94fe8073871a4ea9d9e66f9037e45";
static const uint64_t kTextAddr = 0x10000210c, kTextSize = 0x610ea8;
static const uint64_t kSharedMethod = 0x1000c8d9c;
static const uint64_t kCommVtableTypeInfo = 0x101140100;   // -> "15A3ComponentComm"
static const uint64_t kCommSlot = 0x1011401a8;              // A3ComponentComm vtable +0xa0
static const uint64_t kContentVtable = 0x1011682d8;         // A3EffectEditorContent primary
static const uint64_t kContentCommVtable = 0x1011685c0;     // its A3ComponentComm base
static const uint64_t kContentCommSlot = 0x101168660;       // that vtable's +0xa0
static const size_t kContentCommOffset = 0x118, kUndoOffset = 0x300;
static const size_t kKnobParent = 0x30, kKnobEffect = 0x210, kKnobParameter = 0x212;
static const size_t kKnobDisplayMin = 0x23c, kKnobDisplayMax = 0x240;
static const uint32_t kOpNumericWrite = 0x0009;

// Block instance ID ranges (4 instances each) -> effect-definition cache type,
// from the executable's ID_* symbol tables and its type-name registry
// (name/ID pairs ending at 0x10119fd48: 7 Compressor ... 60 NAM Player).
static const struct { uint16_t first; uint32_t type; } kInstanceTypes[] = {
    {46,7},{50,8},{54,9},{58,10},{62,11},{66,12},{70,13},{74,14},{78,16},{82,17},{86,18},
    {90,19},{94,20},{98,21},{102,40},{106,22},{110,23},{114,24},{118,25},{122,26},{126,28},
    {130,31},{134,32},{138,33},{142,34},{146,35},{150,36},{154,37},{158,38},{162,39},
    {166,50},{170,51},{178,15},{195,55},{202,59},{206,60}};

struct ParameterRecord { uint16_t effect, parameter; uint32_t operation; float raw; uint32_t pad; };
typedef uint64_t (*SharedMethod)(void *self, const void *record, uint32_t flag, void *undo);

static intptr_t gSlide;
static SharedMethod gMethod;

static uint64_t unslide(uintptr_t p) { return (uint64_t)p - gSlide; }
static uintptr_t slid(uint64_t unslid) { return (uintptr_t)(unslid + gSlide); }
static BOOL safeRead(uintptr_t address, void *out, size_t size) {
    mach_vm_size_t got=0;
    return address && mach_vm_read_overwrite(mach_task_self(),(mach_vm_address_t)address,size,(mach_vm_address_t)out,&got)==KERN_SUCCESS && got==size;
}
static uintptr_t readPointer(uintptr_t address) { uintptr_t value=0; return safeRead(address,&value,sizeof value) ? value : 0; }
// Itanium RTTI name of a polymorphic C++ object, or nil (fault-safe).
static NSString *rttiName(uintptr_t object) {
    uintptr_t vtable=readPointer(object), typeInfo=vtable ? readPointer(vtable-8) : 0;
    uintptr_t name=typeInfo ? readPointer(typeInfo+8) & ~(1ULL<<63) : 0;
    char buffer[96]={0};
    if(!name || !safeRead(name,buffer,sizeof buffer-1)) return nil;
    return [NSString stringWithUTF8String:buffer];
}
static uint32_t cacheTypeForInstance(uint16_t effect) {
    for(auto &row:kInstanceTypes) if(effect>=row.first && effect<row.first+4) return row.type;
    return 0;
}

BOOL AxeNativeDisplayToRaw(double display, double rawMin, double rawMax, double scale,
                           double knobMin, double knobMax, float *raw, NSString **failure) {
    NSString *why=nil;
    auto near=[](double a,double b){return std::fabs(a-b)<=1e-3*std::fmax(1.0,std::fmax(std::fabs(a),std::fabs(b)));};
    if(!std::isfinite(display) || !std::isfinite(scale) || scale==0 || !(rawMax>rawMin)) why=@"Parameter has no numeric conversion";
    // The knob's own display range must equal the cache range times its scale;
    // otherwise the display is not linear in the raw value and we refuse.
    else if(!near(knobMin,rawMin*scale) || !near(knobMax,rawMax*scale))
        why=[NSString stringWithFormat:@"Display conversion not verified for this parameter (knob %g..%g, cache %g..%g); not sent",knobMin,knobMax,rawMin*scale,rawMax*scale];
    else if(display<knobMin-1e-9 || display>knobMax+1e-9)
        why=[NSString stringWithFormat:@"Value %g is outside the parameter range %g..%g",display,knobMin,knobMax];
    if(why) {if(failure)*failure=why;return NO;}
    float value=(float)(display/scale);
    *raw=std::fmin(std::fmax(value,(float)rawMin),(float)rawMax);
    return YES;
}

@interface AxeNativeBridge ()
@property(nonatomic, readwrite) BOOL available;
@property(nonatomic, readwrite, copy) NSString *unavailableReason;
@property(nonatomic, copy) NSString *cacheName;
@property(nonatomic, strong) NSDate *cacheDate;
@property(nonatomic, strong) NSDictionary<NSNumber *, NSDictionary<NSNumber *, NSArray *> *> *cache;
@end

@implementation AxeNativeBridge
+ (instancetype)shared {
    static AxeNativeBridge *bridge; static dispatch_once_t once;
    dispatch_once(&once,^{bridge=[AxeNativeBridge new];[bridge verify];});
    return bridge;
}
- (void)verify {
    self.available=NO;
#if !defined(__arm64__)
    self.unavailableReason=@"Native bridge supports only the arm64 slice";
#else
    // DYLD_INSERT_LIBRARIES images precede the executable; find MH_EXECUTE.
    const struct mach_header_64 *header=nullptr;
    for(uint32_t i=0;i<_dyld_image_count();++i) if(_dyld_get_image_header(i)->filetype==MH_EXECUTE) {
        header=(const struct mach_header_64 *)_dyld_get_image_header(i); gSlide=_dyld_get_image_vmaddr_slide(i); break;
    }
    unsigned long size=0;
    uint8_t *text=header ? getsectiondata(header,"__TEXT","__text",&size) : nullptr;
    if(!text || size!=kTextSize || unslide((uintptr_t)text)!=kTextAddr) {self.unavailableReason=@"Executable is not Axe-Edit III 1.15.03 arm64; native writes disabled";return;}
    uint8_t digest[CC_SHA256_DIGEST_LENGTH]; CC_SHA256(text,(CC_LONG)size,digest);
    NSMutableString *hex=[NSMutableString string]; for(int i=0;i<CC_SHA256_DIGEST_LENGTH;++i)[hex appendFormat:@"%02x",digest[i]];
    if(![hex isEqualToString:@(kTextSHA256)]) {self.unavailableReason=@"Executable code hash mismatch; native writes disabled";return;}
    uintptr_t typeInfo=readPointer(slid(kCommVtableTypeInfo));
    char name[32]={0}; uintptr_t namePointer=typeInfo ? readPointer(typeInfo+8) & ~(1ULL<<63) : 0;
    if(!namePointer || !safeRead(namePointer,name,sizeof name-1) || strcmp(name,"15A3ComponentComm")) {self.unavailableReason=@"A3ComponentComm RTTI not found";return;}
    if(readPointer(slid(kCommSlot))!=slid(kSharedMethod) || readPointer(slid(kContentCommSlot))!=slid(kSharedMethod)) {self.unavailableReason=@"Command vtable slot does not target the shared method";return;}
    gMethod=(SharedMethod)slid(kSharedMethod);
    self.available=YES; self.unavailableReason=nil;
#endif
}
// Parse Axe-Edit's own effect-definition cache (the newest one is the one it
// wrote for the connected firmware). Records: id, flags, ?, min, max, scale, step.
- (BOOL)loadCache:(NSString **)failure {
    NSString *dir=[NSHomeDirectory() stringByAppendingPathComponent:@"Library/Application Support/Fractal Audio/Axe-Edit III"];
    NSString *newest=nil; NSDate *newestDate=nil;
    for(NSString *file in [NSFileManager.defaultManager contentsOfDirectoryAtPath:dir error:nil]) {
        if(![file hasPrefix:@"effectDefinitions_10_"] || ![file hasSuffix:@".cache"]) continue;
        NSDate *date=[NSFileManager.defaultManager attributesOfItemAtPath:[dir stringByAppendingPathComponent:file] error:nil].fileModificationDate;
        if(!newestDate || [date compare:newestDate]==NSOrderedDescending) {newest=file;newestDate=date;}
    }
    if(!newest) {if(failure)*failure=@"Axe-Edit effect-definition cache not found";return NO;}
    if(self.cache && [newest isEqual:self.cacheName] && [newestDate isEqual:self.cacheDate]) return YES;
    NSData *data=[NSData dataWithContentsOfFile:[dir stringByAppendingPathComponent:newest]];
    const uint8_t *bytes=(const uint8_t *)data.bytes; size_t length=data.length, pos=0; BOOL ok=YES;
    auto take=[&](void *out,size_t n){ if(pos+n>length){ok=NO;return;} if(out)memcpy(out,bytes+pos,n); pos+=n; };
    uint32_t major=0,minor=0,count=0; uint16_t revision=0;
    take(&major,4);take(&minor,4);take(&revision,2); if(revision>=4)take(nullptr,32); take(&count,4);
    NSMutableDictionary *effects=[NSMutableDictionary dictionary];
    for(uint32_t e=0;ok && e<count;++e) {
        uint32_t type=0,n=0; take(&type,4);take(&n,4);
        NSMutableDictionary *records=[NSMutableDictionary dictionary];
        for(uint32_t r=0;ok && r<n;++r) {
            uint16_t pid=0,flags=0,unknown=0; float a=0,b=0,c=0,d=0; uint32_t labels=0,extra=0;
            take(&pid,2);take(&flags,2);take(&unknown,2);take(&a,4);take(&b,4);take(&c,4);take(&d,4);take(&labels,4);
            for(uint32_t l=0;ok && l<labels;++l) {uint32_t len=0;take(&len,4);take(nullptr,len);}
            take(nullptr,2);take(&extra,4);take(nullptr,4*(size_t)extra);
            records[@(pid)]=@[@(a),@(b),@(c),@(d),@(labels>0),@(flags)];
        }
        effects[@(type)]=records;
    }
    if(!ok || pos!=length) {if(failure)*failure=@"Effect-definition cache did not parse exactly";return NO;}
    self.cache=effects; self.cacheName=newest; self.cacheDate=newestDate;
    return YES;
}
- (NSDictionary *)resolve:(id)node failure:(NSString **)failure {
    NSString *why=nil;
    auto fail=[&](NSString *reason)->NSDictionary *{ if(failure)*failure=reason; return nil; };
    if(!NSThread.isMainThread) return fail(@"Native editor access must run on the main thread");
    if(!self.available) return fail(self.unavailableReason ?: @"Native bridge unavailable");
    Ivar ivar=node ? class_getInstanceVariable([node class],"handler") : nullptr;
    if(!ivar) return fail(@"Control is not a JUCE accessibility element");
    uintptr_t handler=(uintptr_t)*(void **)((uint8_t *)(__bridge void *)node+ivar_getOffset(ivar));
    if(![rttiName(handler) isEqualToString:@"N4juce20AccessibilityHandlerE"]) return fail(@"Accessibility handler type not verified");
    uintptr_t knob=readPointer(handler+8);
    if(![rttiName(knob) isEqualToString:@"12A3RotaryKnob"]) return fail(@"Control is not an Axe-Edit rotary knob; native write not supported");
    uintptr_t content=readPointer(knob+kKnobParent);
    if(readPointer(content)!=slid(kContentVtable)) return fail(@"Knob is not owned by the effect editor");
    uintptr_t comm=content+kContentCommOffset, undo=comm+kUndoOffset;
    if(readPointer(comm)!=slid(kContentCommVtable)) return fail(@"Effect editor command interface not verified");
    if(![rttiName(undo) containsString:@"UndoManager"]) return fail(@"Editor Undo context not verified");
    uint16_t effect=0,parameter=0; float knobMin=0,knobMax=0;
    if(!safeRead(knob+kKnobEffect,&effect,2) || !safeRead(knob+kKnobParameter,&parameter,2) ||
       !safeRead(knob+kKnobDisplayMin,&knobMin,4) || !safeRead(knob+kKnobDisplayMax,&knobMax,4)) return fail(@"Knob identity unreadable");
    // Parameter 0 is the effect model/type selector (special-cased above the
    // shared method with ops 0x3a/0x3b); never treat it as a knob write.
    if(parameter==0) return fail(@"Effect model selection is not supported");
    uint32_t type=cacheTypeForInstance(effect);
    if(!type) return fail([NSString stringWithFormat:@"Block instance %u is not a supported effect",effect]);
    if(![self loadCache:&why]) return fail(why);
    NSArray *record=self.cache[@(type)][@(parameter)];
    if(!record) return fail(@"Parameter not present in the firmware definitions");
    if([record[4] boolValue]) return fail(@"Parameter is a list selection, not a numeric knob");
    return @{@"effect":@(effect),@"parameter":@(parameter),@"effect_type":@(type),
        @"raw_min":record[0],@"raw_max":record[1],@"scale":record[2],@"step":record[3],
        @"display_min":@(knobMin),@"display_max":@(knobMax),@"cache":self.cacheName,
        @"_comm":@((uint64_t)comm),@"_undo":@((uint64_t)undo)};
}
- (NSDictionary *)parameterForAccessibilityNode:(id)node failure:(NSString **)failure {
    NSDictionary *info=[self resolve:node failure:failure];
    if(!info) return nil;
    float raw=0;
    if(!AxeNativeDisplayToRaw([info[@"display_min"] doubleValue],[info[@"raw_min"] doubleValue],[info[@"raw_max"] doubleValue],
        [info[@"scale"] doubleValue],[info[@"display_min"] doubleValue],[info[@"display_max"] doubleValue],&raw,failure)) return nil;
    NSMutableDictionary *visible=[info mutableCopy];
    [visible removeObjectsForKeys:@[@"_comm",@"_undo"]];
    return visible;
}
- (BOOL)setDisplayValue:(double)value forAccessibilityNode:(id)node failure:(NSString **)failure {
    NSDictionary *info=[self resolve:node failure:failure];
    float raw=0;
    if(!info || !AxeNativeDisplayToRaw(value,[info[@"raw_min"] doubleValue],[info[@"raw_max"] doubleValue],[info[@"scale"] doubleValue],
        [info[@"display_min"] doubleValue],[info[@"display_max"] doubleValue],&raw,failure)) return NO;
    ParameterRecord record={(uint16_t)[info[@"effect"] unsignedIntValue],(uint16_t)[info[@"parameter"] unsignedIntValue],kOpNumericWrite,raw,0};
    gMethod((void *)(uintptr_t)[info[@"_comm"] unsignedLongLongValue],&record,0,(void *)(uintptr_t)[info[@"_undo"] unsignedLongLongValue]);
    return YES;
}
@end
