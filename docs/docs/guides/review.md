---
title: Review
sidebar_position: 6
---

# Track changes and comments

Revisions and comments are commands and state like everything else, and the `review` service lists them for your
panes. Your application draws the revisions pane and the comment cards; the engine draws the marks in the document.

```ts file=../../examples/web/review.ts
```

## Authors

Every change and comment is attributed to the session's author: `{ name, initials?, identity? }`, set in the
session options or with `setAuthor`. `identity` (a provider and user id) tells apart two people with the same name.
`review.people()` lists everyone in the document, each with the colour their marks are drawn in; `setAuthorColor`
changes one.

## Tracked changes

`setTrackChanges(true)` records edits as revisions. `state.review.tracking` says whether tracking is on, and
`trackingLocked` whether the document's protection keeps it on. With `trackChanges: true` in the session options,
tracking turns on after each open.

`review.revisions()` lists the revisions with their kind (insertion, deletion, move, formatting, table changes),
author, time, text or description, and page. Accept or reject one (`acceptRevision`, `rejectRevision`), those in
the selection, or all of them, optionally by one author (`acceptAllRevisions({ author })`). `selectRevision('next')`
moves through them. `setMarkupView` chooses what the document shows: `'all'`, `'simple'`, `'noMarkup'` or
`'original'`. `revisionsChanged` says when to refresh the pane.

### Show Markup

`setShowMarkup({ insertionsDeletions?, formatting?, comments? })` chooses which kinds of markup show, and
`setHiddenReviewers({ hidden: [names] })` hides some reviewers' revisions and comments (Show Markup, Specific People;
an empty list shows all reviewers). A revision left out draws as if accepted. `state.review.showMarkup` reads it back,
`review.revisions()` marks each revision `shown`, `review.comments()` each thread and comment `shown`, and
`review.people()` each person `visible`. `state.review.counts` counts the revisions by kind (insertions, deletions,
moves, formatting) and the comment threads twice: `all` of them, and those `shown`.

`acceptAllRevisions({ shown: true })` and `rejectAllRevisions({ shown: true })` resolve only what Show Markup shows
(Accept All Changes Shown); `deleteAllComments({ shownOnly: true })` deletes the threads it shows, whole, by their
first comment (Delete All Comments Shown), and `deleteAllComments()` every comment, resolved ones too.
`acceptAllAndStopTracking()` accepts every revision and turns tracking off, the acceptance one undo step.

### Track Changes Options

`setTrackOptions({ moves?, formatting? })` sets whether tracking records moves as moves and formatting changes: saved
document settings outside the undo history, read back as `state.review.trackMoves` and `trackFormatting`; protection
that forces tracking refuses turning either off. `setMarkupStyle` sets how revisions draw: each kind's `mark`
(`underline`, `bold`, `strikethrough`, ...; `hidden`, `caret` and `pound` for deleted and moved-away text only) and
`color` (`'byAuthor'`, `'auto'` or `{ fixed }`), the change bars' side and colour, and the inserted, deleted, merged
and split cells' fills. What isn't given is kept; `state.review.markupStyle` reads the whole style. It is a view
setting, not saved.

## Comments

`addComment(body)` comments on the selection. A body is plain text or rich text (paragraphs of runs with bold,
italic, underline, strikethrough and links). `replyToComment`, `editComment`, `resolveComment` and `deleteComment`
do the rest, and `setActiveComment` highlights a thread. `review.comments()` lists the threads: the first comment,
its replies, whether it's done, the text it's anchored to and its page. `commentsChanged` says when to refresh.

## Balloons

To show comment cards beside the pages, make room with a markup pane (`markupPaneWidth` when mounting,
`review.setMarkupPaneWidth` after; `MarkupPaneWidth` on `DocumentEditor`). The pages move over to make room; this is
the view's layout, not the document's.

Then either:

- **Let the SDK place them.** `review.attachBalloon(thread, card)` puts your element beside its thread, stacked
  with the others, and keeps it there through scrolling, zooming and editing. `balloonsLaidOut` gives each card's
  position and a leader line to draw. In WinForms, `DocumentEditor.AttachBalloon(thread, control)` places a child
  control, and `PaintMarkupPane` lets you draw the leader lines.
- **Place them yourself.** `commentAnchors` (`CommentAnchorsChanged`) gives each visible thread's position: the
  start and end of its range, its first line's top and bottom, the x where a leader line leaves the page, and
  whether it is active. In the browser these are in the coordinates of `review.balloonLayer`, an element inside the
  scrolling content, so whatever you put there scrolls and zooms with the document. `review.stackBalloons(anchors,
  heights)` computes card tops that don't overlap. The layer has `pointer-events: none`, so clicks reach the page
  through it: give each card `pointer-events: auto` to make it clickable (`attachBalloon` does this for its cards).

`setBalloonMode` chooses where revisions show in All Markup: `'balloons'` (deletions and formatting changes beside
the page), `'commentsAndFormatting'` (the default: formatting changes only) or `'inline'`. The revisions in balloons
come as `revisionBalloons` (`RevisionBalloonsChanged`), anchored as comments are (`pageLeft` and `pageEdge` are the
page's edges); `review.revisions()` gives each one's text and description. One column ordered by page and line top
stacks them with the comment cards.

`review.setBalloonOptions({ side, width, connectingLines, commentColor })` (in .NET, `Review.BalloonOptions`) sets
Track Changes Options' balloon settings for the cards the SDK places: the margin they go in, their preferred width (in
points, or a percent of the page), whether leader lines join them to their text, and the colour comment cards take.
They are the SDK's view settings, not saved; Word's are the default (right, 3.7 in, lines on, by author).
