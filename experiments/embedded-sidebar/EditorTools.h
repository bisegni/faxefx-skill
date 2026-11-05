#import <Cocoa/Cocoa.h>

typedef BOOL (^AxeToolAuthority)(void);
typedef void (^AxeToolReply)(NSDictionary *result);

@interface AxeEditorTools : NSObject
@property(nonatomic, weak) NSView *editor;
+ (instancetype)shared;
+ (NSArray<NSDictionary *> *)specifications;
+ (NSString *)systemPrompt;
- (NSDictionary *)inspect;
- (NSDictionary *)diagnostics;
- (void)call:(NSString *)name arguments:(NSDictionary *)arguments
   authority:(AxeToolAuthority)authority reply:(AxeToolReply)reply;
@end
