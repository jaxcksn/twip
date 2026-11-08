import type { CommentId, Editor } from '@twip/web';

export async function review(editor: Editor) {
  await editor.setAuthor({ name: 'Grace Hopper', initials: 'GH' });
  await editor.commands.setTrackChanges(true);
  await editor.commands.insertText('Tracked text.');

  // The revisions pane: list them, then accept or reject one, or all.
  for (const revision of await editor.review.revisions()) {
    console.log(revision.kind, revision.author.name, revision.text || revision.description);
  }
  const [first] = await editor.review.revisions();
  if (first) await editor.commands.acceptRevision(first.id);
  await editor.commands.rejectAllRevisions();

  // Comments: a thread is its first comment and the replies.
  await editor.commands.selectAll();
  const { id } = await editor.commands.addComment('Is this the final figure?');
  await editor.commands.replyToComment({ to: id, body: 'Yes, from the audited report.' });
  await editor.commands.resolveComment({ id, done: true });
  editor.on('commentsChanged', async () => console.log((await editor.review.comments()).length, 'threads'));
}

// Balloons: room beside the pages, and cards the SDK keeps beside their text through scroll, zoom and edits.
export async function balloons(editor: Editor, card: (thread: CommentId) => HTMLElement) {
  await editor.review.setMarkupPaneWidth(260);
  const attached = new Map<CommentId, () => void>();
  editor.on('commentAnchors', (anchors) => {
    for (const { thread } of anchors) {
      if (!attached.has(thread)) attached.set(thread, editor.review.attachBalloon(thread, card(thread)));
    }
  });
  return () => attached.forEach((detach) => detach());
}
