export interface LinkProps extends React.ComponentProps<"a"> {}

export function Link({ href = "", ...props }: LinkProps) {
  const isExternal = href.startsWith("http");
  const isProtocal = href.startsWith("mailto:") || href.startsWith("tel:");

  return (
    <a
      href={href}
      {...((isProtocal || isExternal) && {
        rel: "noopener noreferrer",
        target: "_blank",
      })}
      {...props}
    />
  );
}
