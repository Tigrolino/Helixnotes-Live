const naturalNameCollator = new Intl.Collator(undefined, { numeric: true });

export function compareNaturalNames(left: string, right: string): number {
  return naturalNameCollator.compare(left, right);
}
