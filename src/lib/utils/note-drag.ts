export function encodeNoteDragPaths(paths: Iterable<string>): string {
  return [...paths].join("\n");
}

export function decodeNoteDragPaths(payload: string): string[] {
  return payload.split(/\r?\n/).filter((path) => path.length > 0);
}
