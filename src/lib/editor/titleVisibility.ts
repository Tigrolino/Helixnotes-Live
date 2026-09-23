export type HiddenTitleHeading = {
	headingPrefix: string;
	title: string;
};

type TitleHeadingResult = {
	markdown: string;
	hiddenTitle: HiddenTitleHeading | null;
};

function normalizeTitle(value: string): string {
	return value.trim().toLowerCase().replace(/[\s\-_\u2014]+/g, ' ');
}

export function stripTitleHeading(
	markdown: string,
	title: string | undefined,
	hideTitle: boolean,
): TitleHeadingResult {
	if (!hideTitle || !title) return { markdown, hiddenTitle: null };

	const lines = markdown.split('\n');
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i].trim();
		if (line === '') continue;

		const match = line.match(/^(#{1,6})\s+(.+)$/);
		if (match && normalizeTitle(match[2]) === normalizeTitle(title)) {
			const hiddenTitle = { headingPrefix: match[1], title: title.trim() };
			lines.splice(i, 1);
			if (i < lines.length && lines[i].trim() === '') lines.splice(i, 1);
			return { markdown: lines.join('\n'), hiddenTitle };
		}
		break;
	}

	return { markdown, hiddenTitle: null };
}

export function restoreTitleHeading(
	markdown: string,
	hiddenTitle: HiddenTitleHeading | null,
): string {
	if (!hiddenTitle) return markdown;
	return `${hiddenTitle.headingPrefix} ${hiddenTitle.title}\n\n${markdown}`;
}
