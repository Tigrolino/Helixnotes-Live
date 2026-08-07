import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';

const CODE_FENCE_PATTERN = /^(?:```|~~~)(?:[a-z]+)?$/;

function isInputRulesPlugin(plugin: Plugin) {
	return 'isInputRules' in plugin.spec && plugin.spec.isInputRules === true;
}

export function createCodeBlockInputScrollPlugin() {
	return new Plugin({
		key: new PluginKey('codeBlockInputScroll'),
		appendTransaction(transactions, oldState, newState) {
			const inputRulesPlugin = oldState.plugins.find(isInputRulesPlugin);
			const wasKeyboardInputRule =
				inputRulesPlugin !== undefined &&
				transactions.some((transaction) => {
					const metadata: unknown = transaction.getMeta(inputRulesPlugin);

					return (
						typeof metadata === 'object' &&
						metadata !== null &&
						'transform' in metadata &&
						metadata.transform === transaction &&
						'text' in metadata &&
						metadata.text === '\n'
					);
				});
			const { selection } = oldState;
			const wasFenceParagraph =
				selection.empty &&
				selection.$from.parent.type.name === 'paragraph' &&
				CODE_FENCE_PATTERN.test(selection.$from.parent.textContent);
			const isCodeBlock = newState.selection.$from.parent.type.name === 'codeBlock';

			return wasKeyboardInputRule && wasFenceParagraph && isCodeBlock
				? newState.tr.scrollIntoView()
				: null;
		},
	});
}

export const CodeBlockInputScroll = Extension.create({
	name: 'codeBlockInputScroll',

	addProseMirrorPlugins() {
		return [createCodeBlockInputScrollPlugin()];
	},
});
