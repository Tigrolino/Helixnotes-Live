function pathParts(path: string): string[] {
	return path.replace(/\\/g, '/').split('/').filter(Boolean);
}

function pathRoot(path: string): string {
	const normalized = path.replace(/\\/g, '/');
	const drive = normalized.match(/^([A-Za-z]:)\//);
	if (drive) return drive[1].toLowerCase();
	return normalized.startsWith('/') ? '/' : '';
}

export function relativePath(fromDirectory: string, targetPath: string): string {
	const from = pathParts(fromDirectory);
	const target = pathParts(targetPath);
	if (pathRoot(fromDirectory) !== pathRoot(targetPath)) return targetPath.replace(/\\/g, '/');
	const windowsPath = /^[A-Za-z]:[\\/]/.test(fromDirectory) || /^[A-Za-z]:[\\/]/.test(targetPath);
	const equal = windowsPath
		? (left: string, right: string) => left.toLowerCase() === right.toLowerCase()
		: (left: string, right: string) => left === right;

	let common = 0;
	while (common < from.length && common < target.length && equal(from[common], target[common])) {
		common++;
	}

	const result = [...Array(from.length - common).fill('..'), ...target.slice(common)];
	return result.join('/') || '.';
}
