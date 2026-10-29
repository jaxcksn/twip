/*
 * The C ABI over twip::api. Internal: the SDKs' private layers call it, and it may change in any release.
 *
 * Structured values cross as UTF-8 JSON in the twip::api types' serde form. A structured response is
 * {"ok": value} or {"error": {"code", "message", "details"}}, written to a TwipBuf the caller frees with
 * twip_buf_free. The int32_t status beside it is 0, or a negated TWIP_E_* code.
 *
 * Every call is made on the engine's thread. Callbacks come after each call's work, the state (kind 2) before any
 * event (kind 1) of the same change, and never re-entrantly: a callback must not call the engine, it queues the work.
 * The payload is valid only during the callback.
 */
#ifndef TWIP_H
#define TWIP_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct TwipEngine TwipEngine;

/* Bytes the engine made; free with twip_buf_free. Zero it before passing it in. */
typedef struct TwipBuf {
    uint8_t* ptr;
    size_t len;
} TwipBuf;

/* What a frame call reports: the page stack's size and the caret, CSS px. */
typedef struct TwipFrameInfo {
    double content_width;
    double content_height;
    double caret_x;
    double caret_y;
    double caret_height;
    uint32_t caret_visible; /* 1 when a caret shows */
    uint32_t pages;         /* laid out so far */
} TwipFrameInfo;

/* Statuses: 0 ok, or one of these (the api ErrorCode order). */
#define TWIP_OK 0
#define TWIP_E_DESTROYED (-1)
#define TWIP_E_INVALID_ARGUMENT (-2)
#define TWIP_E_UNAVAILABLE (-3)
#define TWIP_E_READ_ONLY (-4)
#define TWIP_E_IMPORT_FAILED (-5)
#define TWIP_E_EXPORT_FAILED (-6)
#define TWIP_E_ABORTED (-7)
#define TWIP_E_NOT_ALLOWED (-8)
#define TWIP_E_UNSUPPORTED (-9)
#define TWIP_E_INTERNAL (-10)

/* Callback kinds. */
#define TWIP_KIND_EVENT 1         /* api::Event other than stateChanged (JSON) */
#define TWIP_KIND_STATE 2         /* api::State (JSON) */
#define TWIP_KIND_REQUEST_FRAME 3 /* no payload: draw a frame */
#define TWIP_KIND_SET_TIMER 4     /* {"timer", "ms"}: arm a one-shot timer, then call twip_timer_fired */
#define TWIP_KIND_FONT_REQUEST 5  /* api::FontRequest: resolve, then twip_font_add or FontMarkUnresolved */
#define TWIP_KIND_HOST_REQUEST 6  /* {"type": cursor | capture | release | clipboard | caret | scroll_to | a11y | announce} */

typedef void (*twip_callback)(void* ctx, uint32_t session, uint32_t kind, const uint8_t* ptr, size_t len);

/* Handles and buffers. options_json: {"fonts": "system" | "empty"} or empty. cb is null in the browser, where
 * twip-host.js's twip_emit delivers callbacks. */
TwipEngine* twip_engine_new(const uint8_t* options_json, size_t len, twip_callback cb, void* ctx);
void twip_engine_free(TwipEngine* engine);
/* options_json: an api::SessionOptions, or empty. Returns the session id, 0 on failure. */
uint32_t twip_session_new(TwipEngine* engine, const uint8_t* options_json, size_t len);
void twip_session_free(TwipEngine* engine, uint32_t session);
void twip_buf_free(TwipBuf* buf);
uint8_t* twip_alloc(size_t len);
void twip_free(uint8_t* ptr, size_t len);

/* Structured calls: method numbers. Engine-level methods (catalog, font library, stack balloons) take any session. */
enum TwipMethod {
    TWIP_RUN = 1,                     /* Command -> Commit */
    TWIP_BATCH = 2,                   /* {commands, label?} -> Commit */
    TWIP_STATE = 3,                   /* -> State */
    TWIP_CATALOG = 4,                 /* -> CommandInfo[] */
    TWIP_NEW_DOCUMENT = 5,            /* -> null */
    TWIP_SET_READ_ONLY = 6,           /* bool -> null */
    TWIP_SET_MODE = 7,                /* Mode -> null */
    TWIP_SET_AUTHOR = 8,              /* Author -> null */
    TWIP_CONTEXT_MENU = 9,            /* -> ContextMenu */
    TWIP_INVOKE = 10,                 /* MenuAction -> Commit */
    TWIP_DIALOG_OPEN = 11,            /* DialogKind -> DialogModel */
    TWIP_DIALOG_UPDATE = 12,          /* {model, patch} -> DialogModel */
    TWIP_DIALOG_APPLY = 13,           /* DialogModel -> Commit */
    TWIP_FIND = 14,                   /* {query, direction} -> FindResult */
    TWIP_FIND_ALL = 15,               /* FindQuery -> FindMatch[] */
    TWIP_SELECT_MATCH = 16,           /* MatchId -> null */
    TWIP_REPLACE = 17,                /* {query, with} -> FindResult */
    TWIP_REPLACE_ALL = 18,            /* {query, with} -> {count} */
    TWIP_SPELLING_ADD_DICTIONARY = 19,/* Dictionary -> null */
    TWIP_SPELLING_SET_ENABLED = 20,   /* bool -> null */
    TWIP_SPELLING_SET_LANGUAGE = 21,  /* string -> null */
    TWIP_SPELLING_IGNORE = 22,        /* string -> null */
    TWIP_SPELLING_ADD = 23,           /* string -> null */
    TWIP_SPELLING_LOAD_USER_WORDS = 24, /* {language, words} -> null */
    TWIP_SPELLING_SUGGESTIONS = 25,   /* {word, language?} -> string[] */
    TWIP_SPELLING_NEXT = 26,          /* -> Misspelling | null */
    TWIP_HEADINGS = 27,               /* -> Heading[] */
    TWIP_SELECT_HEADING = 28,         /* HeadingId -> null */
    TWIP_STYLES = 29,                 /* StyleType | null -> StyleInfo[] */
    TWIP_REVISIONS = 30,              /* -> RevisionInfo[] */
    TWIP_COMMENTS = 31,               /* -> CommentThread[] */
    TWIP_PEOPLE = 32,                 /* -> Person[] */
    TWIP_ELEMENTS = 33,               /* ElementFilter | null -> ElementInfo[] */
    TWIP_ELEMENT = 34,                /* ElementId -> ElementDetail */
    TWIP_STRUCTURE = 35,              /* {root?, depth?} | null -> Block[] */
    TWIP_SELECT_ELEMENT = 36,         /* {id, reveal} -> null */
    TWIP_DATA_PARTS = 37,             /* {namespace?, itemId?} | null -> DataPart[] */
    TWIP_ACCESSIBLE_TREE = 38,        /* -> AccessibleNode */
    TWIP_SHORTCUTS_REGISTER = 39,     /* Shortcut -> null */
    TWIP_SHORTCUTS_UNREGISTER = 40,   /* string -> null */
    TWIP_SHORTCUTS_LOOKUP = 41,       /* string -> KeyBinding | null */
    TWIP_GESTURES_BINDINGS = 42,      /* -> GestureBinding[] */
    TWIP_GESTURES_SET = 43,           /* {gesture, action} -> null */
    TWIP_GESTURES_RESET = 44,         /* GestureId | null -> null */
    TWIP_WORD_COUNT = 45,             /* -> WordCount */
    TWIP_COPY = 46,                   /* {cut} -> ClipboardData | null */
    TWIP_PASTE = 47,                  /* {data, asText} -> null */
    TWIP_FONT_FAMILIES = 48,          /* -> string[] */
    TWIP_FONT_SET_SUBSTITUTIONS = 49, /* SubstitutionRule[] -> null */
    TWIP_FONT_SET_DEFAULT = 50,       /* string -> null */
    TWIP_FONT_MARK_UNRESOLVED = 51,   /* FontRequest -> null */
    TWIP_HOST_SET_BACKGROUND = 52,    /* Color -> null */
    TWIP_HOST_SET_MARKUP_PANE_WIDTH = 53, /* number -> null */
    TWIP_HOST_SET_RULERS = 54,        /* {horizontal, vertical} -> null */
    TWIP_HOST_SET_RULER_UNIT = 55,    /* RulerUnit -> null */
    TWIP_HOST_SET_TAB_SELECTOR = 56,  /* TabSelector -> null */
    TWIP_HOST_RULER_VIEW = 57,        /* -> RulerView */
    TWIP_HOST_COMMENT_ANCHORS = 58,   /* -> CommentAnchor[] */
    TWIP_HOST_HIT_TEST = 59,          /* Point -> HitResult */
    TWIP_HOST_DRAG_OVER = 60,         /* Point -> null */
    TWIP_HOST_DRAG_LEAVE = 61,        /* -> null */
    TWIP_HOST_DROP = 62,              /* {at, command} -> Commit */
    TWIP_HOST_REVEAL_SELECTION = 63,  /* -> null */
    TWIP_HOST_VISIBLE_PAGES = 64,     /* -> PageRect[] */
    TWIP_STACK_BALLOONS = 65,         /* {anchors, heights, gap?} -> number[] */
    TWIP_OPEN_COMMIT = 66,            /* token (twip_open_prepare) -> Warning[] */
    TWIP_OPEN_DISCARD = 67,           /* token (twip_open_prepare) -> null */
    TWIP_DIALOG_SELECT_ISSUE = 68,    /* string (an accessibility issue id) -> null */
    TWIP_FONT_SET_RESOLVER = 69,      /* bool: the SDK has a resolver (font requests come as kind 5) -> null */
    TWIP_PLACEHOLDERS = 70            /* -> PlaceholderInfo[] (the unfilled picture placeholders) */
};

int32_t twip_call(TwipEngine* engine, uint32_t session, uint32_t method, const uint8_t* req, size_t len, TwipBuf* out);

/* Bytes in and out. twip_open: opts is {} or empty; out gets {"ok": Warning[]}. twip_export: opts is an
 * api::ExportOptions; the file goes to bytes, raw, and meta gets {"ok": {"revision", "warnings"}}. twip_font_add:
 * out gets {"ok": FaceInfo[]}. */
int32_t twip_open(TwipEngine* engine, uint32_t session, const uint8_t* bytes, size_t len, const uint8_t* opts, size_t olen, TwipBuf* out);
/* A cancellable open: reads the document without replacing the open one; out gets {"ok": token}. TWIP_OPEN_COMMIT
 * swaps it in, TWIP_OPEN_DISCARD drops it; freeing the session drops its tokens. */
int32_t twip_open_prepare(TwipEngine* engine, uint32_t session, const uint8_t* bytes, size_t len, const uint8_t* opts, size_t olen, TwipBuf* out);
int32_t twip_export(TwipEngine* engine, uint32_t session, const uint8_t* opts, size_t olen, TwipBuf* bytes, TwipBuf* meta);
int32_t twip_font_add(TwipEngine* engine, const uint8_t* bytes, size_t len, TwipBuf* out);

/* Hot input. key: a TwipKey; mods: TWIP_MOD_* bits. twip_key returns 0 unhandled, 1 handled, or 2 + n for host
 * command n in api::HostCommand order (save, saveAs, print, open, new, find, replace, goTo, insertHyperlink,
 * nextMisspelling, focusFont, focusFontSize, help). Composition phase: start 0, update 1, end 2. Pointer kind: down
 * 0, move 1, up 2, cancel 3, leave 4; x and y are viewport CSS px; button (the one pressed or released): primary 0,
 * secondary 1, middle 2. */
#define TWIP_MOD_CTRL 1u
#define TWIP_MOD_SHIFT 2u
#define TWIP_MOD_ALT 4u
#define TWIP_MOD_META 8u

/* Keys, by KeyboardEvent.code. */
enum TwipKey {
    TWIP_KEY_A = 1, TWIP_KEY_B, TWIP_KEY_C, TWIP_KEY_D, TWIP_KEY_E, TWIP_KEY_F, TWIP_KEY_G, TWIP_KEY_H, TWIP_KEY_I,
    TWIP_KEY_J, TWIP_KEY_K, TWIP_KEY_L, TWIP_KEY_M, TWIP_KEY_N, TWIP_KEY_O, TWIP_KEY_P, TWIP_KEY_Q, TWIP_KEY_R,
    TWIP_KEY_S, TWIP_KEY_T, TWIP_KEY_U, TWIP_KEY_V, TWIP_KEY_W, TWIP_KEY_X, TWIP_KEY_Y, TWIP_KEY_Z, /* 1..26 */
    TWIP_KEY_DIGIT0 = 27, TWIP_KEY_DIGIT1, TWIP_KEY_DIGIT2, TWIP_KEY_DIGIT3, TWIP_KEY_DIGIT4, TWIP_KEY_DIGIT5,
    TWIP_KEY_DIGIT6, TWIP_KEY_DIGIT7, TWIP_KEY_DIGIT8, TWIP_KEY_DIGIT9, /* 27..36 */
    TWIP_KEY_F1 = 37, TWIP_KEY_F2, TWIP_KEY_F3, TWIP_KEY_F4, TWIP_KEY_F5, TWIP_KEY_F6, TWIP_KEY_F7, TWIP_KEY_F8,
    TWIP_KEY_F9, TWIP_KEY_F10, TWIP_KEY_F11, TWIP_KEY_F12, /* 37..48 */
    TWIP_KEY_ENTER = 61, TWIP_KEY_TAB, TWIP_KEY_BACKSPACE, TWIP_KEY_DELETE, TWIP_KEY_ESCAPE, TWIP_KEY_SPACE,
    TWIP_KEY_INSERT, TWIP_KEY_HOME, TWIP_KEY_END, TWIP_KEY_PAGE_UP, TWIP_KEY_PAGE_DOWN, TWIP_KEY_ARROW_LEFT,
    TWIP_KEY_ARROW_RIGHT, TWIP_KEY_ARROW_UP, TWIP_KEY_ARROW_DOWN, TWIP_KEY_CONTEXT_MENU, /* 61..76 */
    TWIP_KEY_MINUS = 77, TWIP_KEY_EQUAL, TWIP_KEY_BRACKET_LEFT, TWIP_KEY_BRACKET_RIGHT, TWIP_KEY_BACKSLASH,
    TWIP_KEY_SEMICOLON, TWIP_KEY_QUOTE, TWIP_KEY_BACKQUOTE, TWIP_KEY_COMMA, TWIP_KEY_PERIOD, TWIP_KEY_SLASH,
    TWIP_KEY_INTL_BACKSLASH, /* 77..88 */
    TWIP_KEY_NUMPAD_ENTER = 89, TWIP_KEY_NUMPAD_ADD, TWIP_KEY_NUMPAD_SUBTRACT, TWIP_KEY_NUMPAD_MULTIPLY,
    TWIP_KEY_NUMPAD_DIVIDE, TWIP_KEY_NUMPAD_DECIMAL, /* 89..94 */
    TWIP_KEY_NUMPAD0 = 95, TWIP_KEY_NUMPAD1, TWIP_KEY_NUMPAD2, TWIP_KEY_NUMPAD3, TWIP_KEY_NUMPAD4, TWIP_KEY_NUMPAD5,
    TWIP_KEY_NUMPAD6, TWIP_KEY_NUMPAD7, TWIP_KEY_NUMPAD8, TWIP_KEY_NUMPAD9 /* 95..104 */
};

uint32_t twip_key(TwipEngine* engine, uint32_t session, uint32_t key, uint32_t mods);
void twip_text(TwipEngine* engine, uint32_t session, const uint8_t* utf8, size_t len);
void twip_composition(TwipEngine* engine, uint32_t session, uint32_t phase, const uint8_t* utf8, size_t len);
void twip_pointer(TwipEngine* engine, uint32_t session, uint32_t kind, double x, double y, uint32_t button, uint32_t clicks, uint32_t mods);
void twip_wheel(TwipEngine* engine, uint32_t session, double dx, double dy, uint32_t mods);
void twip_focus(TwipEngine* engine, uint32_t session, uint32_t focused);

/* Presentation. A session with a surface (width and height > 0) gets a frame request after each change. */
void twip_surface(TwipEngine* engine, uint32_t session, double width, double height, double dpr);
void twip_scroll(TwipEngine* engine, uint32_t session, double x, double y);
/* CPU pixels: width x height device pixels, RGBA, premultiplied, rows packed. info may be null. */
int32_t twip_frame_rgba(TwipEngine* engine, uint32_t session, uint8_t* pixels, uint32_t width, uint32_t height, TwipFrameInfo* info);
/* Browser GPU frames: WebGPU first, then WebGL 2; twip_frame_rgba is the fallback. Natively these fail
 * TWIP_E_UNAVAILABLE (twip_gpu_instance returns 0). */
uintptr_t twip_gpu_instance(void);
int32_t twip_attach_webgpu(uintptr_t instance, uintptr_t device, uintptr_t queue);
int32_t twip_frame_webgpu(TwipEngine* engine, uint32_t session, uintptr_t texture, uint32_t bgra, uint32_t height, TwipFrameInfo* info);
int32_t twip_attach_webgl(TwipEngine* engine, uint32_t session);
int32_t twip_frame_webgl(TwipEngine* engine, uint32_t session, uint32_t width, uint32_t height, TwipFrameInfo* info);
void twip_timer_fired(TwipEngine* engine, uint32_t session, uint32_t timer);

#ifdef __cplusplus
}
#endif

#endif /* TWIP_H */
