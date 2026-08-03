import type { Node as ProseMirrorNode, Schema } from '@tiptap/pm/model';

export type MixedListName = 'bulletList' | 'taskList';

export function convertListNode(
  schema: Schema,
  listNode: ProseMirrorNode,
  targetListName: MixedListName,
): ProseMirrorNode | null {
  if (
    (listNode.type.name !== 'bulletList' && listNode.type.name !== 'taskList') ||
    listNode.type.name === targetListName
  ) {
    return null;
  }

  const targetListType = schema.nodes[targetListName];
  const targetItemType = schema.nodes[targetListName === 'taskList' ? 'taskItem' : 'listItem'];
  if (!targetListType || !targetItemType) return null;

  const itemAttrs = targetListName === 'taskList' ? { checked: false } : null;
  const convertedItems = listNode.content.content.map((item) =>
    targetItemType.create(itemAttrs, item.content, item.marks)
  );

  return targetListType.create(null, convertedItems, listNode.marks);
}
