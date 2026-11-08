// The engine's callbacks in the browser, as an emscripten --js-library. Each one hands the callback to
// Module.twipDispatch(kind, session, ptr, len), which the SDK registers before its first engine call. A payload is
// readable at HEAPU8[ptr, ptr + len) during the call only; kinds are those of the C ABI (twip.h).
addToLibrary({
  // Events (1), state (2), font (5) and host (6) requests: JSON payloads.
  twip_emit: (kind, session, ptr, len) => Module.twipDispatch(kind, session, ptr, len),

  // Draw a frame (3): no payload.
  twip_request_frame: (session) => Module.twipDispatch(3, session, 0, 0),

  // Arm a one-shot timer (4), then call twip_timer_fired: the payload is { timer, ms }.
  twip_set_timer__deps: ['$withStackSave', '$stringToUTF8OnStack', '$lengthBytesUTF8'],
  twip_set_timer: (session, timer, ms) =>
    withStackSave(() => {
      const json = JSON.stringify({ timer, ms });
      Module.twipDispatch(4, session, stringToUTF8OnStack(json), lengthBytesUTF8(json));
    }),
});
