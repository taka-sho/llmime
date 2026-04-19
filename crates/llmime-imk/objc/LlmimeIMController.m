// Objective-C bridge layer between macOS InputMethodKit and Rust.
// IMKInputController subclass that delegates all state to Rust via C FFI.
#import <InputMethodKit/InputMethodKit.h>
#import <Foundation/Foundation.h>

// ---------------------------------------------------------------------------
// C FFI declarations — implemented in Rust (ffi.rs)
// ---------------------------------------------------------------------------

extern void     llmime_imk_session_begin(uint64_t session_id);
extern void     llmime_imk_session_end(uint64_t session_id);
extern int      llmime_imk_input_text(uint64_t session_id, const char *utf8, uint32_t modifiers);
extern uint32_t llmime_imk_get_candidate_count(uint64_t session_id);
extern void     llmime_imk_get_candidate(uint64_t session_id, uint32_t index, char *buf, uint32_t buf_len);
extern void     llmime_imk_candidate_selected(uint64_t session_id, const char *utf8);
extern void     llmime_imk_candidate_selection_changed(uint64_t session_id, const char *utf8);

// ---------------------------------------------------------------------------
// Session ID helper — object pointer used as a stable unique ID
// ---------------------------------------------------------------------------
static inline uint64_t session_id_of(id obj) {
    return (uint64_t)(uintptr_t)obj;
}

// ---------------------------------------------------------------------------
// LlmimeIMController — IMKInputController subclass
// ---------------------------------------------------------------------------

@interface LlmimeIMController : IMKInputController
@property (nonatomic, strong) IMKCandidates *candidates;
@end

@implementation LlmimeIMController

- (id)initWithServer:(IMKServer *)server delegate:(id)delegate client:(id)inputClient {
    self = [super initWithServer:server delegate:delegate client:inputClient];
    if (self) {
        _candidates = [[IMKCandidates alloc] initWithServer:server
                                                  panelType:kIMKSingleColumnScrollingCandidatePanel];
        llmime_imk_session_begin(session_id_of(self));
    }
    return self;
}

- (void)deactivateServer:(id)sender {
    llmime_imk_session_end(session_id_of(self));
    [self.candidates hide];
    [super deactivateServer:sender];
}

// ---------------------------------------------------------------------------
// Key input
// ---------------------------------------------------------------------------

- (BOOL)inputText:(NSString *)string client:(id)sender {
    const char *utf8 = [string UTF8String];
    if (!utf8) return NO;

    int consumed = llmime_imk_input_text(session_id_of(self), utf8, 0);
    if (consumed) {
        [self updateCandidateWindow:sender];
    }
    return (consumed != 0);
}

- (BOOL)handleEvent:(NSEvent *)event client:(id)sender {
    if ([event type] != NSEventTypeKeyDown) return NO;

    NSString *chars = [event charactersIgnoringModifiers];
    if (!chars || [chars length] == 0) return NO;

    unichar key = [chars characterAtIndex:0];
    if (key == NSDeleteCharacter || key == 0x1B) {
        const char *utf8 = [chars UTF8String];
        int consumed = llmime_imk_input_text(session_id_of(self), utf8,
                                              (uint32_t)[event modifierFlags]);
        if (consumed) {
            [self updateCandidateWindow:sender];
            return YES;
        }
    }
    return NO;
}

// ---------------------------------------------------------------------------
// Candidate window
// ---------------------------------------------------------------------------

- (NSArray *)candidates:(id)sender {
    uint32_t count = llmime_imk_get_candidate_count(session_id_of(self));
    NSMutableArray *result = [NSMutableArray arrayWithCapacity:count];
    char buf[512];
    for (uint32_t i = 0; i < count; i++) {
        llmime_imk_get_candidate(session_id_of(self), i, buf, sizeof(buf));
        [result addObject:[NSString stringWithUTF8String:buf]];
    }
    return [result copy];
}

- (void)candidateSelectionChanged:(NSAttributedString *)candidateString {
    const char *utf8 = [[candidateString string] UTF8String];
    if (utf8) {
        llmime_imk_candidate_selection_changed(session_id_of(self), utf8);
    }
}

- (void)candidateSelected:(NSAttributedString *)candidateString {
    const char *utf8 = [[candidateString string] UTF8String];
    if (utf8) {
        llmime_imk_candidate_selected(session_id_of(self), utf8);
    }
    [self.candidates hide];
}

// ---------------------------------------------------------------------------
// Private
// ---------------------------------------------------------------------------

- (void)updateCandidateWindow:(id)sender {
    NSArray *cands = [self candidates:sender];
    if ([cands count] > 0) {
        [self.candidates updateCandidates];
        [self.candidates show:kIMKLocateCandidatesLeftHint];
    } else {
        [self.candidates hide];
    }
}

@end
