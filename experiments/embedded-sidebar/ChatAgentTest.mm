// Tests execute the same Objective-C++ implementation as the injected panel.
#import "ChatAgent.mm"
#include <assert.h>

static BOOL waitFor(BOOL (^predicate)(void), NSTimeInterval timeout) {
    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout];
    while (!predicate() && deadline.timeIntervalSinceNow > 0)
        [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.02]];
    return predicate();
}

int main(int argc, const char **argv) {
    @autoreleasepool {
        NSString *rootPath = [NSTemporaryDirectory() stringByAppendingPathComponent:
            [@"axe-native-chat-test-" stringByAppendingString:NSUUID.UUID.UUIDString]];
        AxeChatAgent *agent = [[AxeChatAgent alloc] initWithStorageRoot:[NSURL fileURLWithPath:rootPath]];
        if (argc >= 2) {
            BOOL toolsMode=argc>=3 && strcmp(argv[2],"--tools")==0;
            NSString *provider = [NSString stringWithUTF8String:argv[1]];
            [agent connectProvider:provider];
            if (!waitFor(^{ return agent.ready || !agent.connected; }, 50) || !agent.ready) {
                fprintf(stderr, "FAIL connect: %s\n", agent.status.UTF8String); [agent disconnect]; return 1;
            }
            printf("PASS: local %s connected\n", provider.UTF8String); fflush(stdout);
            [agent disconnect];
            [agent connectProvider:provider];
            if (!waitFor(^{ return agent.ready || !agent.connected; }, 50) || !agent.ready) {
                fprintf(stderr, "FAIL empty-session reconnect: %s\n", agent.status.UTF8String); [agent disconnect]; return 1;
            }
            [agent send:toolsMode ? @"Call axefx_inspect exactly once. Do not edit anything. Then reply with exactly TOOLS_INSPECT_OK." : @"This is an integration smoke test for the native Axe-Edit chat panel. Reply with exactly NATIVE_CHAT_OK and nothing else."];
            if (!waitFor(^{ return !agent.busy; }, 120)) {
                fprintf(stderr, "FAIL: reply timeout\n"); [agent disconnect]; return 1;
            }
            BOOL found = NO;
            for (NSDictionary *entry in agent.messages)
                if ([entry[@"role"] isEqual:@"assistant"] && [entry[@"text"] containsString:toolsMode ? @"TOOLS_INSPECT_OK" : @"NATIVE_CHAT_OK"]) found = YES;
            if(toolsMode) {
                BOOL toolCalled=NO;
                for(NSDictionary *entry in agent.messages)if([entry[@"role"] isEqual:@"tool"] && [entry[@"text"] hasPrefix:@"axefx_inspect:"] && [entry[@"tool_result"][@"ok"] boolValue])toolCalled=YES;
                if(!toolCalled){fprintf(stderr,"FAIL: real agent did not invoke the inspection tool\n");[agent disconnect];return 1;}
                printf("PASS: local %s invoked the native inspection tool (no editor attached, no mutations)\n",provider.UTF8String);
            }
            if (!found) {
                fprintf(stderr, "FAIL reply: %s\n", agent.status.UTF8String);
                for (NSDictionary *entry in agent.messages) if ([entry[@"role"] isEqual:@"assistant"])
                    fprintf(stderr, "reply=%s\n", [entry[@"text"] UTF8String]);
                [agent disconnect]; return 1;
            }
            [agent disconnect];
            [agent connectProvider:provider];
            if (!waitFor(^{ return agent.ready || !agent.connected; }, 50) || !agent.ready || agent.messages.count < 2) {
                fprintf(stderr, "FAIL reconnect/history: %s\n", agent.status.UTF8String); [agent disconnect]; return 1;
            }
            [agent disconnect];
            printf("PASS: local %s reply, disconnect, resume and saved history\n", provider.UTF8String);
            return 0;
        }
        agent.provider = @"Codex"; agent.session = @"test-thread"; agent.turn = @"test-turn";
        agent.busy = YES; agent.ready = YES;
        [agent codexEvent:@{@"method":@"item/agentMessage/delta", @"params":@{@"threadId":@"other",@"delta":@"wrong",@"itemId":@"a"}}];
        assert(agent.messages.count == 0);
        NSDictionary *delta = @{@"method":@"item/agentMessage/delta", @"params":@{@"threadId":@"test-thread",@"turnId":@"test-turn",@"itemId":@"a",@"delta":@"hello"}};
        [agent codexEvent:delta];
        [agent codexEvent:@{@"method":@"item/completed", @"params":@{@"threadId":@"test-thread",@"item":@{@"type":@"agentMessage",@"id":@"a",@"text":@"hello"}}}];
        assert(agent.messages.count == 1 && [agent.messages[0][@"text"] isEqual:@"hello"]);
        agent.stopping = YES; [agent codexEvent:delta];
        assert([agent.messages[0][@"text"] isEqual:@"hello"]);
        [agent codexEvent:@{@"method":@"turn/completed",@"params":@{@"threadId":@"test-thread",@"turn":@{@"id":@"test-turn"}}}];
        assert(!agent.busy);
        agent.stopping = NO; agent.claudeMessageID = @"claude-test";
        [agent claudeEvent:@{@"type":@"stream_event",@"event":@{@"delta":@{@"type":@"text_delta",@"text":@"Claude reply"}}}];
        [agent claudeEvent:@{@"type":@"result",@"is_error":@NO,@"result":@"Claude reply"}];
        assert(agent.messages.count == 2 && agent.receivedResult);
        assert([agent.messages[1][@"text"] isEqual:@"Claude reply"]);
        agent.provider=@"Codex";agent.session=@"tool-thread";agent.turn=@"tool-turn";agent.busy=YES;agent.ready=YES;agent.stopping=NO;
        NSUInteger count=agent.messages.count;
        [agent codexEvent:@{@"id":@100,@"method":@"item/tool/call",@"params":@{@"threadId":@"foreign",@"turnId":@"tool-turn",@"tool":@"axefx_inspect",@"arguments":@{}}}];
        assert(agent.messages.count==count);
        [agent codexEvent:@{@"id":@101,@"method":@"item/tool/call",@"params":@{@"threadId":@"tool-thread",@"turnId":@"tool-turn",@"tool":@"axefx_inspect",@"arguments":@{}}}];
        assert(agent.messages.count==count+1 && [agent.messages.lastObject[@"role"] isEqual:@"tool"]);
        agent.stopping=YES;
        [agent codexEvent:@{@"id":@102,@"method":@"item/tool/call",@"params":@{@"threadId":@"tool-thread",@"turnId":@"tool-turn",@"tool":@"axefx_inspect",@"arguments":@{}}}];
        assert(agent.messages.count==count+1);
        printf("PASS: Codex/Claude streaming, final deduplication, foreign-thread rejection stopped-delta rejection and active-turn tool authorization\n");
    }
    return 0;
}
