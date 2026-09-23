import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

const openNoteWindows = new Map<string, string>();

export async function openNoteWindow(notePath: string, noteTitle: string) {
	const existingLabel = openNoteWindows.get(notePath);
	if (existingLabel) {
		const existing = await WebviewWindow.getByLabel(existingLabel);
		if (existing) {
			await existing.show();
			await existing.unminimize();
			await existing.setFocus();
			return;
		}
		openNoteWindows.delete(notePath);
	}

	const label = `note-${Date.now()}`;
	const url = `/?note=${encodeURIComponent(notePath)}`;

	const noteWindow = new WebviewWindow(label, {
		url,
		title: `${noteTitle} - HelixNotes`,
		width: 900,
		height: 700,
		minWidth: 500,
		minHeight: 400,
		decorations: false,
		center: true
	});

	openNoteWindows.set(notePath, label);

	noteWindow.once('tauri://destroyed', () => {
		openNoteWindows.delete(notePath);
	});

	noteWindow.once('tauri://error', (e) => {
		console.error('Failed to create note window:', e);
		openNoteWindows.delete(notePath);
	});
}
