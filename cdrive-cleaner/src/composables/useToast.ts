import { inject, type InjectionKey } from 'vue';
import type { ToastType } from '../types';

export type ShowToastFn = (message: string, subMessage: string, type?: ToastType) => void;

export const TOAST_KEY: InjectionKey<ShowToastFn> = Symbol('toast');

export function useToast(): ShowToastFn {
  const showToast = inject(TOAST_KEY);
  if (!showToast) {
    return (message, subMessage, type = 'info') => {
      console.warn(`[Toast:${type}] ${message} - ${subMessage}`);
    };
  }
  return showToast;
}
