import type { ButtonHTMLAttributes } from 'react';

type HaloButtonProps = ButtonHTMLAttributes<HTMLButtonElement>;

export function HaloButton({ children, className = '', type = 'button', ...props }: HaloButtonProps) {
  return (
    <button
      type={type}
      className={className}
      {...props}
      style={{
        border: '1px solid #24364b',
        borderRadius: '9px',
        background: '#24364b',
        color: '#fff',
        padding: '10px 12px',
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: '8px',
        fontWeight: 600,
        transition: 'background .15s ease, opacity .15s ease',
        ...props.style,
      }}
    >
      {children}
    </button>
  );
}
