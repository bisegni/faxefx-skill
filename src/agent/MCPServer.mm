// Connection-scoped, bearer-authenticated local MCP transport for Claude.
#import "MCPServer.h"
#include <sys/socket.h>
#include <netinet/in.h>
#include <unistd.h>
#include <atomic>
#include <memory>

@interface AxeMCPServer ()
@property(nonatomic, copy) NSString *url;
@property(nonatomic, copy) NSString *token;
@property(nonatomic, copy) AxeMCPHandler handler;
@end
@implementation AxeMCPServer {
    int _listener;
    std::atomic_bool _running;
}
- (instancetype)init { if((self=[super init])) { _listener=-1; _running=false; } return self; }
- (void)respond:(int)client status:(int)status value:(NSDictionary *)value {
    NSData *body=value ? [NSJSONSerialization dataWithJSONObject:value options:0 error:nil] : [NSData data];
    NSString *header=[NSString stringWithFormat:@"HTTP/1.1 %d %@\r\nContent-Type: application/json\r\nContent-Length: %lu\r\nConnection: close\r\n\r\n",status,status==200 ? @"OK" : @"Response",(unsigned long)body.length];
    NSMutableData *bytes=[[header dataUsingEncoding:NSUTF8StringEncoding] mutableCopy];[bytes appendData:body];
    NSUInteger sent=0;
    while(sent<bytes.length) { ssize_t n=send(client,(const char *)bytes.bytes+sent,bytes.length-sent,0);if(n<=0) break;sent+=n; }
}
- (void)serve:(int)client {
    int yes=1;setsockopt(client,SOL_SOCKET,SO_NOSIGPIPE,&yes,sizeof yes);
    timeval timeout={5,0};setsockopt(client,SOL_SOCKET,SO_RCVTIMEO,&timeout,sizeof timeout);setsockopt(client,SOL_SOCKET,SO_SNDTIMEO,&timeout,sizeof timeout);
    NSMutableData *bytes=[NSMutableData data];NSRange separator=NSMakeRange(NSNotFound,0);char chunk[4096];
    NSData *boundary=[@"\r\n\r\n" dataUsingEncoding:NSUTF8StringEncoding];
    while(bytes.length<16384 && separator.location==NSNotFound) {
        ssize_t n=recv(client,chunk,sizeof chunk,0);if(n<=0)return;[bytes appendBytes:chunk length:n];
        separator=[bytes rangeOfData:boundary options:0 range:NSMakeRange(0,bytes.length)];
    }
    if(separator.location==NSNotFound) {[self respond:client status:413 value:nil];return;}
    NSString *headers=[[NSString alloc] initWithData:[bytes subdataWithRange:NSMakeRange(0,separator.location)] encoding:NSUTF8StringEncoding];
    NSArray *lines=[headers componentsSeparatedByString:@"\r\n"];
    if(![lines.firstObject isEqual:@"POST /mcp HTTP/1.1"]) {[self respond:client status:405 value:nil];return;}
    NSMutableDictionary *fields=[NSMutableDictionary dictionary];
    for(NSUInteger i=1;i<lines.count;++i) {NSString *line=lines[i];NSRange colon=[line rangeOfString:@":"];if(colon.location!=NSNotFound)
        fields[[[line substringToIndex:colon.location] lowercaseString]]=[[line substringFromIndex:colon.location+1] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet];}
    if(fields[@"origin"] || ![fields[@"authorization"] isEqual:[@"Bearer " stringByAppendingString:self.token]]) {[self respond:client status:403 value:nil];return;}
    NSString *lengthString=fields[@"content-length"];
    NSScanner *scanner=[NSScanner scannerWithString:lengthString ?: @""];unsigned long long length=0;
    if(![scanner scanUnsignedLongLong:&length] || !scanner.atEnd || length>1024*1024 || fields[@"transfer-encoding"]) {[self respond:client status:413 value:nil];return;}
    NSUInteger offset=NSMaxRange(separator);
    while(bytes.length-offset<length) {ssize_t n=recv(client,chunk,MIN(sizeof chunk,(size_t)(length-(bytes.length-offset))),0);if(n<=0)return;[bytes appendBytes:chunk length:n];}
    id request=[NSJSONSerialization JSONObjectWithData:[bytes subdataWithRange:NSMakeRange(offset,(NSUInteger)length)] options:0 error:nil];
    if(![request isKindOfClass:NSDictionary.class] || ![request[@"method"] isKindOfClass:NSString.class]) {[self respond:client status:400 value:nil];return;}
    NSString *method=request[@"method"];id identifier=request[@"id"] ?: NSNull.null;NSDictionary *result=nil;
    if([method isEqual:@"notifications/initialized"] && !request[@"id"]) {[self respond:client status:202 value:nil];return;}
    if([method isEqual:@"initialize"]) result=@{@"protocolVersion":@"2025-03-26",@"capabilities":@{@"tools":@{}},@"serverInfo":@{@"name":@"axe_edit",@"version":@"2"}};
    else if([method isEqual:@"ping"]) result=@{};
    else if([method isEqual:@"tools/list"]) result=@{@"tools":[AxeEditorTools specifications]};
    else if([method isEqual:@"tools/call"]) {
        NSDictionary *params=request[@"params"];
        if(![params isKindOfClass:NSDictionary.class] || ![params[@"name"] isKindOfClass:NSString.class] || ![params[@"arguments"] isKindOfClass:NSDictionary.class]) {[self respond:client status:400 value:nil];return;}
        __block NSDictionary *reply=nil;dispatch_semaphore_t done=dispatch_semaphore_create(0);
        NSLock *replyLock=[NSLock new];
        auto alive=std::make_shared<std::atomic_bool>(true);
        dispatch_async(dispatch_get_main_queue(), ^{
            if(!alive->load() || !self->_running.load()) {dispatch_semaphore_signal(done);return;}
            self.handler(params[@"name"],params[@"arguments"], ^BOOL {return alive->load() && self->_running.load();}, ^(NSDictionary *value){ [replyLock lock];if(alive->exchange(false))reply=value;[replyLock unlock];dispatch_semaphore_signal(done); });
        });
        if(dispatch_semaphore_wait(done,dispatch_time(DISPATCH_TIME_NOW,10*NSEC_PER_SEC))) {[replyLock lock];alive->store(false);if(!reply)reply=@{@"ok":@NO,@"error":@"Editor request timed out; inspect before any retry"};[replyLock unlock];}
        alive->store(false);
        NSString *text=[[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:reply ?: @{@"ok":@NO,@"error":@"Connection closed"} options:0 error:nil] encoding:NSUTF8StringEncoding];
        result=@{@"isError":@(![reply[@"ok"] boolValue]),@"content":@[@{@"type":@"text",@"text":text ?: @"Tool failed"}]};
    }
    if(result) [self respond:client status:200 value:@{@"jsonrpc":@"2.0",@"id":identifier,@"result":result}];
    else [self respond:client status:200 value:@{@"jsonrpc":@"2.0",@"id":identifier,@"error":@{@"code":@(-32601),@"message":@"Unknown application method"}}];
}
- (BOOL)startWithHandler:(AxeMCPHandler)handler error:(NSError **)error {
    self.handler=handler;self.token=[NSUUID.UUID.UUIDString stringByAppendingString:NSUUID.UUID.UUIDString];
    int fd=socket(AF_INET,SOCK_STREAM,0);sockaddr_in address={};address.sin_family=AF_INET;address.sin_addr.s_addr=htonl(INADDR_LOOPBACK);address.sin_port=0;
    if(fd<0 || bind(fd,(sockaddr *)&address,sizeof address)!=0 || listen(fd,8)!=0) {if(fd>=0)close(fd);if(error)*error=[NSError errorWithDomain:@"AxeMCP" code:1 userInfo:@{NSLocalizedDescriptionKey:@"Cannot open local agent tool endpoint"}];return NO;}
    socklen_t size=sizeof address;getsockname(fd,(sockaddr *)&address,&size);_listener=fd;_running=true;
    self.url=[NSString stringWithFormat:@"http://127.0.0.1:%u/mcp",ntohs(address.sin_port)];
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED,0), ^{
        while(self->_running.load()) {@autoreleasepool {int client=accept(fd,NULL,NULL);if(client<0)break;if(self->_running.load())[self serve:client];close(client);}}
    });
    return YES;
}
- (void)stop { _running=false;if(_listener>=0){shutdown(_listener,SHUT_RDWR);close(_listener);_listener=-1;} }
- (void)dealloc {if(_listener>=0)close(_listener);}
@end
