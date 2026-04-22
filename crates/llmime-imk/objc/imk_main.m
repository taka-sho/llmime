#import <InputMethodKit/InputMethodKit.h>
#import <Foundation/Foundation.h>

// Entry point for the llmime input method server process.
// Initialises IMKServer using the bundle's Info.plist connection name,
// then hands control to NSRunLoop for the lifetime of the process.
void llmime_imk_run_main(void) {
    @autoreleasepool {
        NSString *connectionName = [[NSBundle mainBundle]
            objectForInfoDictionaryKey:@"InputMethodConnectionName"];
        if (!connectionName) {
            connectionName = @"com.takasho.llmime_Connection";
        }
        NSString *bundleId = [[NSBundle mainBundle] bundleIdentifier];
        if (!bundleId) {
            bundleId = @"com.takasho.llmime";
        }

        IMKServer *server = [[IMKServer alloc]
            initWithName:connectionName
            bundleIdentifier:bundleId];
        (void)server;

        [[NSRunLoop mainRunLoop] run];
    }
}
