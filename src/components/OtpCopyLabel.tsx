import { Check, Copy } from 'lucide-react';

export function OtpCopyLabel({ copied, label = 'Copy OTP' }: { copied: boolean; label?: string }) {
  const Icon = copied ? Check : Copy;
  return (
    <>
      <Icon className="w-3.5 h-3.5" aria-hidden="true" />
      <span>{copied ? 'Copied' : label}</span>
    </>
  );
}
