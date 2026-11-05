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
        if (argc == 2) {
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
            [agent send:@"This is an integration smoke test for the native Axe-Edit chat panel. Reply with exactly NATIVE_CHAT_OK and nothing else."];
            if (!waitFor(^{ return !agent.busy; }, 120)) {
                fprintf(stderr, "FAIL: reply timeout\n"); [agent disconnect]; return 1;
            }
            BOOL found = NO;
            for (NSDictionary *entry in agent.messages)
                if ([entry[@"role"] isEqual:@"assistant"] && [entry[@"text"] containsString:@"NATIVE_CHAT_OK"]) found = YES;
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
        printf("PASS: Codex/Claude streaming, final deduplication, foreign-thread rejection and stopped-delta rejection\n");
    }
    return 0;
}
