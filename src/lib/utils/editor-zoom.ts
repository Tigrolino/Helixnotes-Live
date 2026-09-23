export type WheelFontSizeAction = 'ignore' | 'block' | 'increase' | 'decrease';

interface WheelModifierInput {
  ctrlKey: boolean;
  metaKey: boolean;
  deltaY: number;
}

export function getWheelFontSizeAction(
  event: WheelModifierInput,
  scrollToChangeFontSize: boolean,
): WheelFontSizeAction {
  if (!event.ctrlKey && !event.metaKey) return 'ignore';
  if (!scrollToChangeFontSize) return 'block';
  return event.deltaY < 0 ? 'increase' : 'decrease';
}
