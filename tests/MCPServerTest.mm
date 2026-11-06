#import "MCPServer.h"
#include <assert.h>

static NSDictionary *request(AxeMCPServer *server,NSString *token,NSString *origin,NSDictionary *body) {
    NSMutableURLRequest *request=[NSMutableURLRequest requestWithURL:[NSURL URLWithString:server.url]];
    request.HTTPMethod=@"POST";request.HTTPBody=[NSJSONSerialization dataWithJSONObject:body options:0 error:nil];
    if(token)[request setValue:[@"Bearer " stringByAppendingString:token] forHTTPHeaderField:@"Authorization"];
    if(origin)[request setValue:origin forHTTPHeaderField:@"Origin"];
    [request setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
    __block NSDictionary *result=nil;
    [[NSURLSession.sharedSession dataTaskWithRequest:request completionHandler:^(NSData *data,NSURLResponse *response,NSError *error){
        NSDictionary *json=data.length ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : @{};
        dispatch_async(dispatch_get_main_queue(),^{result=@{@"status":@([(NSHTTPURLResponse *)response statusCode]),@"body":json ?: @{}};});
    }] resume];
    NSDate *deadline=[NSDate dateWithTimeIntervalSinceNow:15];
    while(!result && deadline.timeIntervalSinceNow>0)[NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
    assert(result);return result;
}
int main(void){@autoreleasepool{
    AxeMCPServer *server=[AxeMCPServer new];__block NSUInteger calls=0;NSError *error=nil;
    assert([server startWithHandler:^(NSString *name,NSDictionary *args,AxeToolAuthority alive,AxeToolReply reply){assert(NSThread.isMainThread && alive());calls++;reply(@{@"ok":@YES});} error:&error]);
    NSDictionary *list=@{@"jsonrpc":@"2.0",@"id":@1,@"method":@"tools/list"};
    assert([request(server,nil,nil,list)[@"status"] integerValue]==403);
    assert([request(server,@"wrong",nil,list)[@"status"] integerValue]==403);
    assert([request(server,server.token,@"https://example.com",list)[@"status"] integerValue]==403);
    NSDictionary *result=request(server,server.token,nil,list);assert([result[@"status"] integerValue]==200 && [result[@"body"][@"result"][@"tools"] count]==10 && calls==0);
    NSDictionary *call=@{@"jsonrpc":@"2.0",@"id":@2,@"method":@"tools/call",@"params":@{@"name":@"axefx_inspect",@"arguments":@{}}};
    result=request(server,server.token,nil,call);assert(calls==1 && ![result[@"body"][@"result"][@"isError"] boolValue]);
    NSString *oldToken=server.token;[server stop];
    AxeMCPServer *fresh=[AxeMCPServer new];
    assert([fresh startWithHandler:^(NSString *name,NSDictionary *args,AxeToolAuthority alive,AxeToolReply reply){calls++;reply(@{@"ok":@YES});} error:&error]);
    assert(![fresh.token isEqual:oldToken]);assert([request(fresh,oldToken,nil,call)[@"status"] integerValue]==403 && calls==1);
    [fresh stop];puts("PASS: local MCP authentication, browser Origin rejection, tool discovery/call, main-thread dispatch and old-token rejection");
}return 0;}
