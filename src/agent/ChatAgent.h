#import <Foundation/Foundation.h>

@interface AxeChatAgent : NSObject
- (instancetype)initWithStorageRoot:(NSURL *)root;
@property(nonatomic, copy) void (^changed)(void);
@property(nonatomic, readonly) BOOL ready;
@property(nonatomic, readonly) BOOL busy;
@property(nonatomic, readonly) BOOL connected;
@property(nonatomic, readonly, copy) NSString *status;
@property(nonatomic, readonly) NSArray<NSDictionary *> *messages;
- (void)connectProvider:(NSString *)provider;
- (void)send:(NSString *)text;
- (void)stop;
- (void)disconnect;
@end
