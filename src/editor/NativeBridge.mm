// See NativeBridge.h. Addresses are unslid ARM64 VAs.
//
// Verified chain for Axe-Edit III 1.15.03 (2026-11-05, live, read-only probes
// plus one authorized write/restore of Amp 1 Gain 2.51 -> 2.61 -> 2.51):
//   AX slider (JUCEAccessibilityElement) .handler -> juce::AccessibilityHandler
//   handler+0x08 -> A3RotaryKnob: +0x210 u16 effect ID, +0x212 u16 parameter ID,
//                   +0x23c/+0x240 float display min/max, +0x30 parent component
//   parent = A3EffectEditorContent (primary vtable 0x1011682d8)
//   +0x118 = its A3ComponentComm base (vtable 0x1011685c0); slot +0xa0 is the
//            shared command method 0x1000c8d9c(this, record, flag, undo)
//   comm+0x300 = the editor's Undo context (seen on every user edit)
//   A typed entry calls it with {effect, parameter, op 0x09, raw float}, flag 0.
// scripts/discover_axe_edit.py derives the static part of this chain for any
// arm64 build; the runtime offsets are calibrated by AxeCalibrateKnobLayout.
#import "NativeBridge.h"
#import <AppKit/AppKit.h>
#import <CommonCrypto/CommonDigest.h>
#include <mach-o/dyld.h>
#include <mach-o/getsect.h>
#include <mach/mach.h>
#include <mach/mach_vm.h>
#include <malloc/malloc.h>
#include <objc/runtime.h>
#include <cmath>
#include <cstring>
#include <set>

static const uint32_t kOpNumericWrite = 0x0009;
static NSString *const kEnabledPrefix = @"AxeBridgeWritesEnabled.";

struct Profile {
    uint64_t textAddr, textSize; NSString *textSHA;
    uint64_t method, commTypeInfoSlot, commSlot, contentVtable, contentCommVtable, contentCommSlot;
    long contentCommOffset;
    // Runtime layout (-1 = calibrate).
    long knobParent, knobEffect, knobParameter, knobMin, knobMax, undoOffset;
    bool verified;
};
static Profile builtInProfile() {
    return {0x10000210c, 0x610ea8, @"e7aa716d8c58501a43408ab2643d1f51a3b94fe8073871a4ea9d9e66f9037e45",
        0x1000c8d9c, 0x101140100, 0x1011401a8, 0x1011682d8, 0x1011685c0, 0x101168660, 0x118,
        0x30, 0x210, 0x212, 0x23c, 0x240, 0x300, true};
}

// Block instance ID ranges (4 instances each) -> effect-definition cache type,
// from the executable's ID_* symbol tables and its type-name registry
// (name/ID pairs: 7 Compressor ... 60 NAM Player). These are firmware IDs,
// stable across editor builds.
static const struct { uint16_t first; uint32_t type; } kInstanceTypes[] = {
    {46,7},{50,8},{54,9},{58,10},{62,11},{66,12},{70,13},{74,14},{78,16},{82,17},{86,18},
    {90,19},{94,20},{98,21},{102,40},{106,22},{110,23},{114,24},{118,25},{122,26},{126,28},
    {130,31},{134,32},{138,33},{142,34},{146,35},{150,36},{154,37},{158,38},{162,39},
    {166,50},{170,51},{178,15},{195,55},{202,59},{206,60}};

struct ParameterRecord { uint16_t effect, parameter; uint32_t operation; float raw; uint32_t pad; };
typedef uint64_t (*SharedMethod)(void *self, const void *record, uint32_t flag, void *undo);

static intptr_t gSlide;
static SharedMethod gMethod;

static void bridgeLog(NSString *message) {
    NSString *path = [NSHomeDirectory() stringByAppendingPathComponent:@"Library/Logs/AxeEditSidebarProof.log"];
    NSData *data = [[NSString stringWithFormat:@"%@ bridge: %@\n", [NSDate date], message] dataUsingEncoding:NSUTF8StringEncoding];
    NSFileHandle *file = [NSFileHandle fileHandleForWritingAtPath:path];
    if (!file) [data writeToFile:path atomically:YES]; else { [file seekToEndOfFile]; [file writeData:data]; [file closeFile]; }
}
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
static bool nearValue(double a, double b) { return std::fabs(a-b)<=1e-3*std::fmax(1.0,std::fmax(std::fabs(a),std::fabs(b))); }

BOOL AxeNativeDisplayToRaw(double display, double rawMin, double rawMax, double scale,
                           double knobMin, double knobMax, float *raw, NSString **failure) {
    NSString *why=nil;
    if(!std::isfinite(display) || !std::isfinite(scale) || scale==0 || !(rawMax>rawMin)) why=@"Parameter has no numeric conversion";
    // The knob's own display range must equal the cache range times its scale;
    // otherwise the display is not linear in the raw value and we refuse.
    else if(!nearValue(knobMin,rawMin*scale) || !nearValue(knobMax,rawMax*scale))
        why=[NSString stringWithFormat:@"Display conversion not verified for this parameter (knob %g..%g, cache %g..%g); not sent",knobMin,knobMax,rawMin*scale,rawMax*scale];
    else if(display<knobMin-1e-9 || display>knobMax+1e-9)
        why=[NSString stringWithFormat:@"Value %g is outside the parameter range %g..%g",display,knobMin,knobMax];
    if(why) {if(failure)*failure=why;return NO;}
    float value=(float)(display/scale);
    *raw=std::fmin(std::fmax(value,(float)rawMin),(float)rawMax);
    return YES;
}

NSDictionary *AxeCalibrateKnobLayout(NSArray<NSNumber *> *knobs, uintptr_t contentVtable,
    uint32_t (^typeForInstance)(uint16_t), NSArray *(^recordFor)(uint32_t, uint16_t), NSString **failure) {
    auto fail=[&](NSString *why)->NSDictionary *{ if(failure)*failure=why; return nil; };
    if(knobs.count<3) return fail(@"Open an effect with at least three knobs to calibrate");
    std::vector<uintptr_t> k; for(NSNumber *n in knobs) k.push_back((uintptr_t)n.unsignedLongLongValue);
    // 1. Parent pointer: same A3EffectEditorContent for every knob.
    long parent=-1;
    for(long off=8;off<0x200 && parent<0;off+=8) {
        uintptr_t first=readPointer(k[0]+off);
        if(!first || readPointer(first)!=contentVtable) continue;
        bool all=true; for(uintptr_t knob:k) if(readPointer(knob+off)!=first) {all=false;break;}
        if(all) parent=off;
    }
    if(parent<0) return fail(@"Knob parent editor not found");
    // 2. IDs: a shared u16 block instance followed by a u16 parameter that the
    //    firmware definitions know for every knob, with distinct parameters.
    long ids=-1; uint32_t type=0;
    for(long off=0x40;off<0x800 && ids<0;off+=2) {
        uint16_t effect=0; if(!safeRead(k[0]+off,&effect,2)) break;
        uint32_t t=typeForInstance(effect); if(!t) continue;
        std::set<uint16_t> distinct; bool ok=true;
        for(uintptr_t knob:k) {
            uint16_t e=0,p=0;
            if(!safeRead(knob+off,&e,2) || !safeRead(knob+off+2,&p,2) || e!=effect || p==0 || !recordFor(t,p)) {ok=false;break;}
            distinct.insert(p);
        }
        if(ok && distinct.size()>=2) {ids=off;type=t;}
    }
    if(ids<0) return fail(@"Knob parameter IDs not found");
    // 3. Display range: float pair equal to the definition's raw range x scale
    //    for every numeric knob.
    long range=-1;
    for(long off=0x40;off<0x800 && range<0;off+=4) {
        if(off>=ids-6 && off<=ids+4) continue;
        int matched=0; bool ok=true;
        for(uintptr_t knob:k) {
            uint16_t p=0; float lo=0,hi=0;
            safeRead(knob+ids+2,&p,2);
            NSArray *r=recordFor(type,p);
            if([r[4] boolValue]) continue;   // list parameter: no numeric range
            if(!safeRead(knob+off,&lo,4) || !safeRead(knob+off+4,&hi,4)) {ok=false;break;}
            double scale=[r[2] doubleValue];
            if(!nearValue(lo,[r[0] doubleValue]*scale) || !nearValue(hi,[r[1] doubleValue]*scale)) {ok=false;break;}
            ++matched;
        }
        if(ok && matched>=2) range=off;
    }
    if(range<0) return fail(@"Knob display range not found");
    return @{@"knob_parent":@(parent),@"knob_effect":@(ids),@"knob_parameter":@(ids+2),@"knob_min":@(range),@"knob_max":@(range+4)};
}

long AxeFindUndoOffset(uintptr_t base, size_t limit) {
    long found=-1;
    for(long off=0x100;off+8<=(long)std::min<size_t>(limit,0x800);off+=8)
        if([rttiName(base+off) containsString:@"UndoManager"]) { if(found>=0) return -1; found=off; }   // must be unique
    return found;
}

@interface AxeNativeBridge () {
    Profile _profile;
}
@property(nonatomic, readwrite) BOOL available;
@property(nonatomic, readwrite, copy) NSString *unavailableReason;
@property(nonatomic, readwrite, copy) NSString *profileKind;
@property(nonatomic, copy) NSString *cacheName;
@property(nonatomic, strong) NSDate *cacheDate;
@property(nonatomic, strong) NSDictionary<NSNumber *, NSDictionary<NSNumber *, NSArray *> *> *cache;
@property(nonatomic, copy) NSString *calibrationFailure;
@property(nonatomic) BOOL checkedBuiltInLayout;
@end

@implementation AxeNativeBridge
+ (instancetype)shared {
    static AxeNativeBridge *bridge; static dispatch_once_t once;
    dispatch_once(&once,^{bridge=[AxeNativeBridge new];[bridge verify];});
    return bridge;
}
// Profile shipped by the patcher for this copy, or nil.
- (NSDictionary *)shippedProfile {
    NSURL *url = [NSBundle.mainBundle URLForResource:@"AxeBridgeProfile" withExtension:@"json"];
    NSData *data = url ? [NSData dataWithContentsOfURL:url] : nil;
    id json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
    return [json isKindOfClass:NSDictionary.class] && [json[@"schema"] isEqual:@1] ? json : nil;
}
- (void)verify {
    self.available=NO; self.profileKind=nil;
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
    if(!text) {self.unavailableReason=@"Executable code not found; native writes disabled";return;}
    uint8_t digest[CC_SHA256_DIGEST_LENGTH]; CC_SHA256(text,(CC_LONG)size,digest);
    NSMutableString *hex=[NSMutableString string]; for(int i=0;i<CC_SHA256_DIGEST_LENGTH;++i)[hex appendFormat:@"%02x",digest[i]];
    // Pick the profile whose code identity matches this executable exactly.
    Profile profile=builtInProfile();
    NSString *kind=@"verified-1.15.03";
    // Developer check: AXE_BRIDGE_FORCE_DISCOVERED=1 makes a verified build use
    // its shipped discovered profile and runtime calibration, to exercise the
    // new-build path against known-good values.
    BOOL forceDiscovered = getenv("AXE_BRIDGE_FORCE_DISCOVERED") && [self shippedProfile];
    if(![hex isEqualToString:profile.textSHA] || forceDiscovered) {
        NSDictionary *shipped=[self shippedProfile];
        if(![shipped[@"text"][@"sha256"] isEqual:hex]) {self.unavailableReason=@"This Axe-Edit build has no matching bridge profile; native writes disabled (re-run scripts/patch-axe-edit)";return;}
        auto u=[&](NSString *key)->uint64_t{ return [shipped[key] unsignedLongLongValue]; };
        profile={[shipped[@"text"][@"address"] unsignedLongLongValue],[shipped[@"text"][@"size"] unsignedLongLongValue],hex,
            u(@"command_method"),u(@"comm_typeinfo_slot"),u(@"comm_slot"),u(@"content_vtable"),u(@"content_comm_vtable"),
            u(@"content_comm_slot"),(long)u(@"content_comm_offset"),-1,-1,-1,-1,-1,-1,false};
        kind=@"discovered";
    }
    if(size!=profile.textSize || unslide((uintptr_t)text)!=profile.textAddr) {self.unavailableReason=@"Executable layout does not match its profile; native writes disabled";return;}
    uintptr_t typeInfo=readPointer(slid(profile.commTypeInfoSlot));
    char name[32]={0}; uintptr_t namePointer=typeInfo ? readPointer(typeInfo+8) & ~(1ULL<<63) : 0;
    if(!namePointer || !safeRead(namePointer,name,sizeof name-1) || strcmp(name,"15A3ComponentComm")) {self.unavailableReason=@"A3ComponentComm RTTI not found";return;}
    if(readPointer(slid(profile.commSlot))!=slid(profile.method) || readPointer(slid(profile.contentCommSlot))!=slid(profile.method)) {self.unavailableReason=@"Command vtable slot does not target the shared method";return;}
    _profile=profile;
    gMethod=(SharedMethod)slid(profile.method);
    self.available=YES; self.unavailableReason=nil; self.profileKind=kind;
    bridgeLog([NSString stringWithFormat:@"profile %@ method=0x%llx", kind, profile.method]);
#endif
}
- (BOOL)layoutKnown {
    return _profile.knobParent>=0 && _profile.knobEffect>=0 && _profile.knobMin>=0 && _profile.undoOffset>=0;
}
- (BOOL)calibrationSettled {
    return !self.available || ([self layoutKnown] && (!_profile.verified || self.checkedBuiltInLayout));
}
- (BOOL)needsUserEnable {
    return self.available && !_profile.verified &&
        ![NSUserDefaults.standardUserDefaults boolForKey:[kEnabledPrefix stringByAppendingString:_profile.textSHA ?: @""]];
}
- (void)enableWritesForThisBuild {
    if(!self.available || _profile.verified) return;
    [NSUserDefaults.standardUserDefaults setBool:YES forKey:[kEnabledPrefix stringByAppendingString:_profile.textSHA]];
    bridgeLog(@"user enabled knob writes for this discovered build");
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
static uintptr_t knobForNode(id node) {
    Ivar ivar=node ? class_getInstanceVariable([node class],"handler") : nullptr;
    if(!ivar) return 0;
    uintptr_t handler=(uintptr_t)*(void **)((uint8_t *)(__bridge void *)node+ivar_getOffset(ivar));
    if(![rttiName(handler) isEqualToString:@"N4juce20AccessibilityHandlerE"]) return 0;
    uintptr_t knob=readPointer(handler+8);
    return [rttiName(knob) isEqualToString:@"12A3RotaryKnob"] ? knob : 0;
}
- (void)calibrateWithAccessibilityNodes:(NSArray *)nodes {
    if(!self.available || !NSThread.isMainThread) return;
    BOOL known=[self layoutKnown];
    if(known && self.checkedBuiltInLayout) return;
    NSMutableArray *knobs=[NSMutableArray array];
    for(id node in nodes) { uintptr_t knob=knobForNode(node); if(knob) [knobs addObject:@((uint64_t)knob)]; }
    if(knobs.count<3) return;
    NSString *why=nil;
    if(![self loadCache:&why]) { self.calibrationFailure=why; return; }
    NSDictionary *cache=self.cache;
    NSDictionary *layout=AxeCalibrateKnobLayout(knobs,slid(_profile.contentVtable),^uint32_t(uint16_t e){return cacheTypeForInstance(e);},
        ^NSArray *(uint32_t t,uint16_t p){return cache[@(t)][@(p)];},&why);
    long undo=-1;
    if(layout) {
        uintptr_t content=readPointer((uintptr_t)[knobs[0] unsignedLongLongValue]+[layout[@"knob_parent"] longValue]);
        // Scan only inside the editor's own heap allocation.
        size_t allocation=malloc_size((const void *)content);
        size_t limit=allocation>(size_t)_profile.contentCommOffset ? allocation-_profile.contentCommOffset : 0;
        undo=limit ? AxeFindUndoOffset(content+_profile.contentCommOffset,limit) : -1;
        if(undo<0) {layout=nil; why=@"Editor Undo context not found";}
    }
    if(known) {
        // Verified build: calibration must agree with the hand-verified layout.
        self.checkedBuiltInLayout=YES;
        BOOL agrees=layout && [layout[@"knob_parent"] longValue]==_profile.knobParent && [layout[@"knob_effect"] longValue]==_profile.knobEffect &&
            [layout[@"knob_min"] longValue]==_profile.knobMin && undo==_profile.undoOffset;
        bridgeLog([NSString stringWithFormat:@"runtime calibration %@ the verified layout (%@)", agrees ? @"agrees with" : @"DIFFERS from", layout ?: why]);
        return;
    }
    if(!layout) { self.calibrationFailure=why; bridgeLog([@"calibration failed: " stringByAppendingString:why ?: @"?"]); return; }
    _profile.knobParent=[layout[@"knob_parent"] longValue]; _profile.knobEffect=[layout[@"knob_effect"] longValue];
    _profile.knobParameter=[layout[@"knob_parameter"] longValue]; _profile.knobMin=[layout[@"knob_min"] longValue];
    _profile.knobMax=[layout[@"knob_max"] longValue]; _profile.undoOffset=undo;
    self.calibrationFailure=nil;
    bridgeLog([NSString stringWithFormat:@"calibrated layout %@ undo=0x%lx", layout, undo]);
}
- (NSDictionary *)resolve:(id)node failure:(NSString **)failure {
    NSString *why=nil;
    auto fail=[&](NSString *reason)->NSDictionary *{ if(failure)*failure=reason; return nil; };
    if(!NSThread.isMainThread) return fail(@"Native editor access must run on the main thread");
    if(!self.available) return fail(self.unavailableReason ?: @"Native bridge unavailable");
    if(![self layoutKnown]) return fail(self.calibrationFailure ?: @"Knob layout not calibrated yet: select an effect with several knobs");
    if(self.needsUserEnable) return fail(@"This Axe-Edit build was auto-detected; knob writes stay off until the user enables them in the panel");
    uintptr_t knob=knobForNode(node);
    if(!knob) return fail(@"Control is not an Axe-Edit rotary knob; native write not supported");
    uintptr_t content=readPointer(knob+_profile.knobParent);
    if(readPointer(content)!=slid(_profile.contentVtable)) return fail(@"Knob is not owned by the effect editor");
    uintptr_t comm=content+_profile.contentCommOffset, undo=comm+_profile.undoOffset;
    if(readPointer(comm)!=slid(_profile.contentCommVtable)) return fail(@"Effect editor command interface not verified");
    if(![rttiName(undo) containsString:@"UndoManager"]) return fail(@"Editor Undo context not verified");
    uint16_t effect=0,parameter=0; float knobMin=0,knobMax=0;
    if(!safeRead(knob+_profile.knobEffect,&effect,2) || !safeRead(knob+_profile.knobParameter,&parameter,2) ||
       !safeRead(knob+_profile.knobMin,&knobMin,4) || !safeRead(knob+_profile.knobMax,&knobMax,4)) return fail(@"Knob identity unreadable");
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
