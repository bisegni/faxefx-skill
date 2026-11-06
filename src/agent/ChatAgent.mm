// Native local-agent transport: Codex App Server (JSON-RPC over stdio) and Claude stream-json with a private loopback MCP server.
#import "ChatAgent.h"
#import "EditorTools.h"
#import "MCPServer.h"
#include <sys/stat.h>

static NSArray<NSString *> *disabledFeatures(void) {
    return @[@"shell_tool", @"unified_exec", @"code_mode", @"multi_agent", @"multi_agent_v2",
        @"apps", @"plugins", @"hooks", @"browser_use", @"computer_use", @"image_generation", @"request_permissions_tool"];
}

@interface AxeChatAgent ()
@property(nonatomic) BOOL ready;
@property(nonatomic) BOOL busy;
@property(nonatomic) BOOL connected;
@property(nonatomic, copy) NSString *status;
@property(nonatomic, strong) NSMutableArray<NSMutableDictionary *> *entries;
@property(nonatomic, strong) NSTask *task;
@property(nonatomic, strong) NSFileHandle *input;
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, id> *pending;
@property(nonatomic) NSUInteger requestID;
@property(nonatomic) NSUInteger generation;
@property(nonatomic, copy) NSString *provider;
@property(nonatomic, copy) NSString *session;
@property(nonatomic, copy) NSString *turn;
@property(nonatomic, copy) NSString *executable;
@property(nonatomic, strong) NSURL *directory;
@property(nonatomic, strong) NSURL *storageRoot;
@property(nonatomic) BOOL receivedResult;
@property(nonatomic) BOOL stopping;
@property(nonatomic, copy) NSString *claudeMessageID;
@property(nonatomic, copy) NSDictionary *threadParams;
@property(nonatomic, strong) NSNumber *resumeRequestID;
@property(nonatomic, strong) AxeMCPServer *mcp;
@property(nonatomic, strong) NSURL *mcpConfig;
@property(nonatomic) NSUInteger toolEpoch;
@property(nonatomic) NSUInteger toolCount;
@end

@implementation AxeChatAgent
- (instancetype)init {
    NSURL *support = [NSFileManager.defaultManager URLsForDirectory:NSApplicationSupportDirectory inDomains:NSUserDomainMask].firstObject;
    return [self initWithStorageRoot:[support URLByAppendingPathComponent:@"AxeStudioEmbeddedChat"]];
}
- (instancetype)initWithStorageRoot:(NSURL *)root {
    if ((self = [super init])) {
        _storageRoot = root;
        _entries = [NSMutableArray array];
        _pending = [NSMutableDictionary dictionary];
        _status = @"Choose your local agent and connect.";
        _provider = @"Codex";
    }
    return self;
}
- (NSArray *)messages { return self.entries; }
- (void)notify { if (self.changed) self.changed(); }
- (void)fail:(NSString *)message {
    [self disconnect]; self.status = message; [self notify];
}
- (NSMutableDictionary *)entry:(NSString *)identifier role:(NSString *)role {
    for (NSMutableDictionary *entry in self.entries)
        if ([entry[@"id"] isEqual:identifier]) return entry;
    NSMutableDictionary *entry = [@{@"id":identifier, @"role":role, @"text":@"", @"status":@"streaming"} mutableCopy];
    [self.entries addObject:entry];
    return entry;
}
- (void)persist {
    if (!self.directory) return;
    NSDictionary *state = @{@"version":@3, @"provider":self.provider, @"session":self.session ?: @"", @"messages":self.entries};
    NSData *data = [NSJSONSerialization dataWithJSONObject:state options:0 error:nil];
    NSURL *file = [self.directory URLByAppendingPathComponent:@"conversation.json"];
    NSError *error = nil;
    if (![data writeToURL:file options:NSDataWritingAtomic error:&error]) {
        self.status = @"Conversation could not be saved locally.";
    } else chmod(file.fileSystemRepresentation, 0600);
}
- (NSString *)findExecutable:(NSString *)name {
    NSMutableArray *paths = [NSMutableArray arrayWithArray:[NSProcessInfo.processInfo.environment[@"PATH"] componentsSeparatedByString:@":"] ?: @[]];
    [paths addObjectsFromArray:@[@"/opt/homebrew/bin", @"/usr/local/bin", [NSHomeDirectory() stringByAppendingPathComponent:@".local/bin"]]];
    for (NSString *path in paths) {
        NSString *file = [path stringByAppendingPathComponent:name];
        if ([NSFileManager.defaultManager isExecutableFileAtPath:file]) return file;
    }
    return nil;
}
- (NSDictionary *)environment {
    NSMutableDictionary *env = [NSProcessInfo.processInfo.environment mutableCopy];
    for (NSString *key in env.allKeys)
        if ([key hasPrefix:@"DYLD_"]) [env removeObjectForKey:key];
    // The native panel uses the CLI's local login, never supplied API keys.
    [env removeObjectsForKeys:@[@"OPENAI_API_KEY", @"ANTHROPIC_API_KEY", @"ANTHROPIC_AUTH_TOKEN"]];
    return env;
}
- (void)launch:(NSArray<NSString *> *)arguments
          line:(void (^)(NSDictionary *))line ended:(void (^)(int))ended {
    NSTask *task = [NSTask new];
    task.executableURL = [NSURL fileURLWithPath:self.executable];
    task.arguments = arguments;
    task.environment = [self environment];
    task.currentDirectoryURL = self.directory;
    NSPipe *output = [NSPipe pipe], *input = [NSPipe pipe];
    task.standardOutput = output;
    task.standardInput = input;
    task.standardError = NSFileHandle.fileHandleWithNullDevice;
    NSError *error = nil;
    if (![task launchAndReturnError:&error]) { [self fail:error.localizedDescription]; return; }
    self.task = task;
    self.input = input.fileHandleForWriting;
    __weak AxeChatAgent *weakSelf = self;
    NSUInteger generation = self.generation;
    BOOL jsonDocument = [arguments.firstObject isEqual:@"auth"];
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        NSMutableData *buffer = [NSMutableData data];
        @try {
            while (YES) {
                NSData *chunk = output.fileHandleForReading.availableData;
                if (!chunk.length) break;
                [buffer appendData:chunk];
                if (buffer.length > 4 * 1024 * 1024) { [task terminate]; break; }
                if (jsonDocument) continue;
                while (YES) {
                    NSRange newline = [buffer rangeOfData:[NSData dataWithBytes:"\n" length:1] options:0 range:NSMakeRange(0, buffer.length)];
                    if (newline.location == NSNotFound) break;
                    NSData *frame = [buffer subdataWithRange:NSMakeRange(0, newline.location)];
                    [buffer replaceBytesInRange:NSMakeRange(0, newline.location + 1) withBytes:NULL length:0];
                    id value = [NSJSONSerialization JSONObjectWithData:frame options:0 error:nil];
                    if ([value isKindOfClass:NSDictionary.class]) dispatch_async(dispatch_get_main_queue(), ^{
                        AxeChatAgent *agent = weakSelf;
                        if (agent && agent.generation == generation && agent.task == task) line(value);
                    });
                }
            }
            // Auth status may produce one JSON document without a trailing newline.
            if (buffer.length) {
                id value = [NSJSONSerialization JSONObjectWithData:buffer options:0 error:nil];
                if ([value isKindOfClass:NSDictionary.class]) dispatch_async(dispatch_get_main_queue(), ^{
                    AxeChatAgent *agent = weakSelf;
                    if (agent && agent.generation == generation && agent.task == task) line(value);
                });
            }
        } @catch (NSException *exception) { /* Closed pipe during disconnect. */ }
        [task waitUntilExit];
        int status = task.terminationStatus;
        dispatch_async(dispatch_get_main_queue(), ^{
            AxeChatAgent *agent = weakSelf;
            if (agent && agent.generation == generation && agent.task == task) {
                agent.task = nil; agent.input = nil; ended(status);
            }
        });
    });
}
- (BOOL)write:(NSDictionary *)value {
    NSMutableData *data = [[NSJSONSerialization dataWithJSONObject:value options:0 error:nil] mutableCopy];
    [data appendBytes:"\n" length:1];
    @try { [self.input writeData:data]; return self.input != nil; }
    @catch (NSException *exception) { [self fail:@"Local agent pipe closed. Reconnect to continue."]; return NO; }
}
- (void)request:(NSString *)method params:(NSDictionary *)params reply:(void (^)(NSDictionary *))reply {
    NSNumber *identifier = @(++self.requestID);
    if (reply) self.pending[identifier] = [reply copy];
    [self write:@{@"id":identifier, @"method":method, @"params":params}];
    NSUInteger generation = self.generation;
    __weak AxeChatAgent *weakSelf = self;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        AxeChatAgent *agent = weakSelf;
        if (agent.generation == generation && agent.pending[identifier])
            [agent fail:@"Local agent did not answer within 30 seconds. Reconnect to retry."];
    });
}
- (void)connectProvider:(NSString *)provider {
    [self disconnect];
    self.provider = [provider isEqual:@"Claude"] ? @"Claude" : @"Codex";
    self.executable = [self findExecutable:self.provider.lowercaseString];
    if (!self.executable) { [self fail:[NSString stringWithFormat:@"Install the local %@ CLI, then reconnect.", self.provider]]; return; }
    self.directory = [self.storageRoot URLByAppendingPathComponent:self.provider];
    NSError *error = nil;
    if (![NSFileManager.defaultManager createDirectoryAtURL:self.directory withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0700} error:&error]) {
        [self fail:@"Cannot create local conversation storage."]; return;
    }
    NSData *saved = [NSData dataWithContentsOfURL:[self.directory URLByAppendingPathComponent:@"conversation.json"]];
    id state = saved ? [NSJSONSerialization JSONObjectWithData:saved options:NSJSONReadingMutableContainers error:nil] : nil;
    self.entries = [NSMutableArray array];
    if ([state isKindOfClass:NSDictionary.class] && [state[@"messages"] isKindOfClass:NSArray.class]) {
        for (id entry in state[@"messages"]) if ([entry isKindOfClass:NSDictionary.class] &&
            [entry[@"id"] isKindOfClass:NSString.class] && [entry[@"text"] isKindOfClass:NSString.class]) {
            NSMutableDictionary *copy = [entry mutableCopy];
            if ([copy[@"status"] isEqual:@"streaming"]) copy[@"status"] = @"interrupted";
            [self.entries addObject:copy];
        }
    }
    self.session = [state isKindOfClass:NSDictionary.class] && [state[@"session"] isKindOfClass:NSString.class] ? state[@"session"] : nil;
    // Start a new agent session on the tool-enabled protocol upgrade. Retain
    // the local transcript, rather than resuming a chat-only tool contract.
    if(![state[@"version"] isEqual:@3]) self.session=nil;
    self.connected = YES; self.busy = YES; self.ready = NO;
    self.status = [NSString stringWithFormat:@"Connecting to local %@…", self.provider]; [self notify];
    if ([self.provider isEqual:@"Codex"]) [self connectCodex];
    else [self connectClaude];
}
- (void)connectCodex {
    NSMutableArray *arguments = [NSMutableArray arrayWithArray:@[@"app-server", @"--stdio"]];
    for (NSString *feature in disabledFeatures()) [arguments addObjectsFromArray:@[@"-c", [NSString stringWithFormat:@"features.%@=false", feature]]];
    [arguments addObjectsFromArray:@[@"-c", @"features.code_mode_host=true", @"-c", @"web_search=\"disabled\""]];
    __weak AxeChatAgent *weakSelf = self;
    [self launch:arguments line:^(NSDictionary *value) { [weakSelf codexEvent:value]; }
        ended:^(int code) { [weakSelf fail:@"Codex disconnected. Reconnect to continue."]; }];
    if (!self.task) return;
    [self request:@"initialize" params:@{@"clientInfo":@{@"name":@"axe_edit_sidebar",@"version":@"0.1.0"}, @"capabilities":@{@"experimentalApi":@YES}} reply:^(NSDictionary *result) {
        AxeChatAgent *agent = weakSelf;
        [agent write:@{@"method":@"initialized", @"params":@{}}];
        [agent request:@"config/read" params:@{@"includeLayers":@NO} reply:^(NSDictionary *loaded) {
            NSMutableDictionary *config = [@{@"web_search":@"disabled"} mutableCopy];
            for (NSString *feature in disabledFeatures()) {
                if (![loaded[@"config"][@"features"][feature] isEqual:@NO]) { [weakSelf fail:@"Codex did not confirm disabled built-in tools."]; return; }
                config[[NSString stringWithFormat:@"features.%@", feature]] = @NO;
            }
            NSDictionary *servers = loaded[@"config"][@"mcp_servers"];
            if ([servers isKindOfClass:NSDictionary.class]) for (NSString *name in servers)
                config[[NSString stringWithFormat:@"mcp_servers.%@.enabled", name]] = @NO;
            [weakSelf request:@"account/read" params:@{@"refreshToken":@NO} reply:^(NSDictionary *account) {
                if ([account[@"requiresOpenaiAuth"] isEqual:@YES] && ![account[@"account"] isKindOfClass:NSDictionary.class]) {
                    [weakSelf fail:@"Sign in using codex login, then reconnect."]; return;
                }
                if ([account[@"account"] isKindOfClass:NSDictionary.class] && [account[@"account"][@"type"] isEqual:@"apiKey"]) {
                    [weakSelf fail:@"Use codex login with your account; this panel uses the local agent login."]; return;
                }
                [weakSelf request:@"skills/list" params:@{@"cwds":@[weakSelf.directory.path], @"forceReload":@YES} reply:^(NSDictionary *listed) {
                    NSMutableArray *skills = [NSMutableArray array];
                    for (NSDictionary *cwd in listed[@"data"]) for (NSDictionary *skill in cwd[@"skills"])
                        if ([skill[@"path"] isKindOfClass:NSString.class]) [skills addObject:@{@"path":skill[@"path"], @"enabled":@NO}];
                    config[@"skills.config"] = skills;
                    NSMutableArray *tools=[NSMutableArray array];
                    for(NSDictionary *spec in [AxeEditorTools specifications]) {NSMutableDictionary *tool=[spec mutableCopy];tool[@"type"]=@"function";[tools addObject:tool];}
                    NSString *instructions=[AxeEditorTools systemPrompt];
                    NSMutableDictionary *params = [@{@"cwd":weakSelf.directory.path, @"sandbox":@"read-only", @"approvalPolicy":@"never", @"config":config,
                        @"environments":@[], @"dynamicTools":tools, @"baseInstructions":instructions, @"developerInstructions":instructions} mutableCopy];
                    BOOL resume = weakSelf.session.length > 0;
                    if (resume) { params[@"threadId"] = weakSelf.session; params[@"excludeTurns"] = @YES; }
                    weakSelf.threadParams = params;
                    weakSelf.resumeRequestID = resume ? @(weakSelf.requestID + 1) : nil;
                    [weakSelf request:resume ? @"thread/resume" : @"thread/start" params:params reply:^(NSDictionary *started) {
                        if (![started[@"sandbox"][@"type"] isEqual:@"readOnly"] || ![started[@"thread"][@"id"] isKindOfClass:NSString.class]) {
                            [weakSelf fail:@"Codex did not confirm a read-only conversation."]; return;
                        }
                        weakSelf.session = started[@"thread"][@"id"]; weakSelf.ready = YES; weakSelf.busy = NO;
                        weakSelf.status = @"Connected to local Codex"; [weakSelf persist]; [weakSelf notify];
                    }];
                }];
            }];
        }];
    }];
}
- (void)codexEvent:(NSDictionary *)value {
    if (value[@"id"] && !value[@"method"]) {
        void (^reply)(NSDictionary *) = self.pending[value[@"id"]];
        [self.pending removeObjectForKey:value[@"id"]];
        if ([value[@"error"] isKindOfClass:NSDictionary.class]) {
            NSString *message = value[@"error"][@"message"] ?: @"Codex request failed.";
            if ([value[@"id"] isEqual:self.resumeRequestID] && self.entries.count == 0 &&
                [message containsString:@"no rollout found"]) {
                NSMutableDictionary *fresh = [self.threadParams mutableCopy];
                [fresh removeObjectsForKeys:@[@"threadId", @"excludeTurns"]];
                self.session = nil; self.resumeRequestID = nil;
                [self request:@"thread/start" params:fresh reply:reply];
                return;
            }
            [self fail:message]; return;
        }
        if (reply) reply(value[@"result"] ?: @{});
        return;
    }
    if (value[@"id"] && value[@"method"]) {
        NSDictionary *params=value[@"params"];
        if([value[@"method"] isEqual:@"item/tool/call"] && [params[@"threadId"] isEqual:self.session] && [params[@"turnId"] isEqual:self.turn]) {
            NSUInteger generation=self.generation;
            __weak AxeChatAgent *weakSelf=self;
            [self invokeTool:params[@"tool"] arguments:params[@"arguments"] extraAuthority:^BOOL{return YES;} reply:^(NSDictionary *result) {
                if(weakSelf.generation!=generation)return;
                NSString *text=[[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:result options:0 error:nil] encoding:NSUTF8StringEncoding];
                [weakSelf write:@{@"id":value[@"id"],@"result":@{@"success":@([result[@"ok"] boolValue]),@"contentItems":@[@{@"type":@"inputText",@"text":text ?: @"Tool failed"}]}}];
            }];
        } else [self write:@{@"id":value[@"id"], @"error":@{@"code":@(-32601), @"message":@"Unknown tool, inactive turn or unavailable approval"}}];
        return;
    }
    NSString *method = value[@"method"];
    NSDictionary *params = value[@"params"];
    if (![params isKindOfClass:NSDictionary.class]) return;
    if (params[@"threadId"] && ![params[@"threadId"] isEqual:self.session]) return;
    if (params[@"turnId"] && ![params[@"turnId"] isEqual:self.turn]) return;
    if ([method isEqual:@"item/agentMessage/delta"] && self.busy && !self.stopping) {
        NSMutableDictionary *entry = [self entry:params[@"itemId"] ?: @"assistant" role:@"assistant"];
        if ([params[@"delta"] isKindOfClass:NSString.class] && [entry[@"text"] length] < 2 * 1024 * 1024)
            entry[@"text"] = [entry[@"text"] stringByAppendingString:params[@"delta"]];
        [self persist]; [self notify];
    } else if ([method isEqual:@"item/completed"] && [params[@"item"][@"type"] isEqual:@"agentMessage"] && !self.stopping) {
        NSMutableDictionary *entry = [self entry:params[@"item"][@"id"] ?: @"assistant" role:@"assistant"];
        entry[@"text"] = params[@"item"][@"text"] ?: @""; entry[@"status"] = @"complete"; [self persist]; [self notify];
    } else if ([method isEqual:@"turn/completed"] && [params[@"turn"][@"id"] isEqual:self.turn]) {
        BOOL failed = [params[@"turn"][@"error"] isKindOfClass:NSDictionary.class];
        self.status = failed ? params[@"turn"][@"error"][@"message"] : (self.stopping ? @"Stopped · partial reply retained" : @"Connected to local Codex");
        self.busy = NO; self.stopping = NO; self.turn = nil; [self persist]; [self notify];
    }
}
- (void)connectClaude {
    __weak AxeChatAgent *weakSelf = self;
    __block BOOL loggedIn = NO;
    [self launch:@[@"auth", @"status", @"--json"] line:^(NSDictionary *value) {
        loggedIn = [value[@"loggedIn"] isEqual:@YES] && [value[@"authMethod"] isEqual:@"claude.ai"];
    } ended:^(int code) {
        if (code != 0 || !loggedIn) { [weakSelf fail:@"Sign in using claude auth login, then reconnect."]; return; }
        if(![weakSelf configureMCP])return;
        weakSelf.ready = YES; weakSelf.busy = NO; weakSelf.status = @"Connected to local Claude"; [weakSelf notify];
    }];
}
- (BOOL)configureMCP {
    [self.mcp stop];
    if(self.mcpConfig)[NSFileManager.defaultManager removeItemAtURL:self.mcpConfig error:nil];
    __weak AxeChatAgent *weakSelf=self;
        self.mcp=[AxeMCPServer new];NSError *error=nil;
        if(![self.mcp startWithHandler:^(NSString *name,NSDictionary *args,AxeToolAuthority authority,AxeToolReply reply) {
            [weakSelf invokeTool:name arguments:args extraAuthority:authority reply:reply];
        } error:&error]) {[self fail:error.localizedDescription];return NO;}
        self.mcpConfig=[self.directory URLByAppendingPathComponent:[NSString stringWithFormat:@"tools-%@.json",NSUUID.UUID.UUIDString]];
        NSDictionary *config=@{@"mcpServers":@{@"axe_edit":@{@"type":@"http",@"url":self.mcp.url,@"headers":@{@"Authorization":[@"Bearer " stringByAppendingString:self.mcp.token]}}}};
        NSData *data=[NSJSONSerialization dataWithJSONObject:config options:0 error:nil];
        if(![data writeToURL:self.mcpConfig options:NSDataWritingAtomic error:&error]) {[self fail:@"Cannot write private local tool configuration"];return NO;}
        chmod(self.mcpConfig.fileSystemRepresentation,0600);
    return YES;
}
- (void)send:(NSString *)text {
    text = [text stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    if (!self.ready || self.busy || !text.length || text.length > 12000) return;
    NSMutableDictionary *entry = [self entry:NSUUID.UUID.UUIDString role:@"user"];
    entry[@"text"] = text; entry[@"status"] = @"submitted";
    self.busy = YES; self.stopping = NO; self.status = @"Thinking…"; [self persist]; [self notify];
    self.toolEpoch++;self.toolCount=0;
    __weak AxeChatAgent *weakSelf = self;
    if ([self.provider isEqual:@"Codex"]) {
        [self request:@"turn/start" params:@{@"threadId":self.session, @"input":@[@{@"type":@"text", @"text":text}]} reply:^(NSDictionary *result) {
            weakSelf.turn = result[@"turn"][@"id"];
            if (weakSelf.stopping && weakSelf.turn) [weakSelf stop];
        }];
    } else {
        // Each Claude turn gets fresh loopback authority, so a stopped or old
        // child cannot reuse its bearer token during a later turn.
        if(![self configureMCP])return;
        self.claudeMessageID = NSUUID.UUID.UUIDString; self.receivedResult = NO;
        NSMutableArray *args = [NSMutableArray arrayWithArray:@[@"--restricted", @"--print", @"--verbose", @"--output-format", @"stream-json", @"--include-partial-messages",
            @"--tools", @"", @"--strict-mcp-config", @"--mcp-config", self.mcpConfig.path, @"--setting-sources", @"", @"--disable-slash-commands",
            @"--settings", @"{\"disableAllHooks\":true}", @"--permission-mode", @"dontAsk", @"--permission-prompts", @"none", @"--no-chrome", @"--max-turns", @"12", @"--system-prompt-snapshot", @"off", @"--system-prompt", [AxeEditorTools systemPrompt]]];
        NSMutableArray *allowed=[NSMutableArray array];for(NSDictionary *spec in [AxeEditorTools specifications]) [allowed addObject:[@"mcp__axe_edit__" stringByAppendingString:spec[@"name"]]];
        [args addObjectsFromArray:@[@"--allowedTools",[allowed componentsJoinedByString:@","]]];
        if (self.session.length) [args addObjectsFromArray:@[@"--resume", self.session]];
        [self launch:args line:^(NSDictionary *value) { [weakSelf claudeEvent:value]; } ended:^(int code) {
            if (weakSelf.stopping) weakSelf.status = @"Stopped · partial reply retained";
            else if (code != 0 || !weakSelf.receivedResult) weakSelf.status = @"Claude ended without a reply. Reconnect if the local login expired.";
            else weakSelf.status = @"Connected to local Claude";
            weakSelf.busy = NO; weakSelf.stopping = NO; [weakSelf persist]; [weakSelf notify];
        }];
        @try { [self.input writeData:[text dataUsingEncoding:NSUTF8StringEncoding]]; [self.input closeFile]; }
        @catch (NSException *exception) { [self fail:@"Claude input pipe closed."]; }
    }
}
- (void)claudeEvent:(NSDictionary *)value {
    if (self.stopping) return;
    if ([value[@"session_id"] isKindOfClass:NSString.class]) self.session = value[@"session_id"];
    if ([value[@"type"] isEqual:@"stream_event"] && [value[@"event"][@"delta"][@"type"] isEqual:@"text_delta"]) {
        NSMutableDictionary *entry = [self entry:self.claudeMessageID role:@"assistant"];
        NSString *delta = value[@"event"][@"delta"][@"text"];
        if ([delta isKindOfClass:NSString.class] && [entry[@"text"] length] < 2 * 1024 * 1024) entry[@"text"] = [entry[@"text"] stringByAppendingString:delta];
    } else if ([value[@"type"] isEqual:@"result"]) {
        self.receivedResult = ![value[@"is_error"] isEqual:@YES];
        NSMutableDictionary *entry = [self entry:self.claudeMessageID role:@"assistant"];
        if ([value[@"result"] isKindOfClass:NSString.class]) entry[@"text"] = value[@"result"];
        entry[@"status"] = self.receivedResult ? @"complete" : @"failed";
    }
    [self persist]; [self notify];
}
- (void)stop {
    if (!self.busy) return;
    if (!self.ready) { [self disconnect]; return; }
    self.toolEpoch++;
    self.stopping = YES; self.status = @"Stopping…"; [self notify];
    for (NSMutableDictionary *entry in self.entries) if ([entry[@"status"] isEqual:@"streaming"]) entry[@"status"] = @"interrupted";
    if ([self.provider isEqual:@"Codex"]) {
        if (self.turn) [self request:@"turn/interrupt" params:@{@"threadId":self.session, @"turnId":self.turn} reply:^(NSDictionary *result) {}];
    } else if (self.task.running) [self.task terminate];
    [self persist];
}
- (void)disconnect {
    self.toolEpoch++;
    [self.mcp stop];self.mcp=nil;
    if(self.mcpConfig) [NSFileManager.defaultManager removeItemAtURL:self.mcpConfig error:nil];self.mcpConfig=nil;
    self.generation++;
    [self.pending removeAllObjects];
    NSTask *task = self.task; self.task = nil;
    @try { [self.input closeFile]; } @catch (NSException *exception) {}
    self.input = nil;
    if (task.running) [task terminate];
    for (NSMutableDictionary *entry in self.entries) if ([entry[@"status"] isEqual:@"streaming"]) entry[@"status"] = @"interrupted";
    [self persist];
    self.ready = NO; self.busy = NO; self.connected = NO; self.stopping = NO; self.turn = nil;
    self.threadParams = nil; self.resumeRequestID = nil;
    self.status = @"Disconnected · conversation saved locally"; [self notify];
}
// Start a fresh conversation: forget the transcript and the provider session
// (so the next turn opens a new thread instead of resuming), then reconnect if
// the agent was connected. Refused while a reply is running.
- (BOOL)clear {
    if (self.busy) return NO;
    BOOL wasConnected = self.connected; NSString *provider = self.provider;
    if (wasConnected) [self disconnect];
    if (!self.directory) self.directory = [self.storageRoot URLByAppendingPathComponent:provider];
    [NSFileManager.defaultManager createDirectoryAtURL:self.directory withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0700} error:nil];
    [self.entries removeAllObjects];
    self.session = nil;
    [self persist];
    if (wasConnected) [self connectProvider:provider];
    else { self.status = @"Conversation cleared"; [self notify]; }
    return YES;
}
- (void)dealloc {
    [self.mcp stop];
    if(self.mcpConfig) [NSFileManager.defaultManager removeItemAtURL:self.mcpConfig error:nil];
    @try { [self.input closeFile]; } @catch (NSException *exception) {}
    if (self.task.running) [self.task terminate];
}
- (void)invokeTool:(NSString *)name arguments:(NSDictionary *)arguments extraAuthority:(AxeToolAuthority)extra reply:(AxeToolReply)reply {
    if(![name isKindOfClass:NSString.class] || ![arguments isKindOfClass:NSDictionary.class] || !self.ready || !self.busy || self.stopping || !extra()) {reply(@{@"ok":@NO,@"error":@"No active guitar-tone turn or tool authority revoked"});return;}
    if(++self.toolCount>32) {reply(@{@"ok":@NO,@"error":@"Tool limit reached for this turn. Summarize results and ask the user to listen."});return;}
    NSUInteger epoch=self.toolEpoch,generation=self.generation;
    NSMutableDictionary *args=[arguments mutableCopy];
    if([args[@"operation_id"] isKindOfClass:NSString.class]) args[@"operation_id"]=[NSString stringWithFormat:@"%@/%@",self.session ?: self.provider,args[@"operation_id"]];
    __weak AxeChatAgent *weakSelf=self;
    [[AxeEditorTools shared] call:name arguments:args authority:^BOOL {
        return weakSelf && weakSelf.generation==generation && weakSelf.toolEpoch==epoch && weakSelf.busy && !weakSelf.stopping && extra();
    } reply:^(NSDictionary *result) {
        if(weakSelf.generation==generation) {
            NSMutableDictionary *entry=[weakSelf entry:NSUUID.UUID.UUIDString role:@"tool"];
            entry[@"text"]=[NSString stringWithFormat:@"%@: %@",name,result[@"status"] ?: result[@"error"] ?: ([result[@"ok"] boolValue] ? @"inspected" : @"failed")];
            entry[@"status"]=@"complete";entry[@"tool_result"]=result;[weakSelf persist];[weakSelf notify];
        }
        reply(result);
    }];
}
@end
