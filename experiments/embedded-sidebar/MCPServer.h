#import <Foundation/Foundation.h>
#import "EditorTools.h"
typedef void (^AxeMCPHandler)(NSString *, NSDictionary *, AxeToolAuthority, AxeToolReply);

@interface AxeMCPServer : NSObject
@property(nonatomic, readonly, copy) NSString *url;
@property(nonatomic, readonly, copy) NSString *token;
- (BOOL)startWithHandler:(AxeMCPHandler)handler error:(NSError **)error;
- (void)stop;
@end
