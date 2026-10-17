import type { ComponentProps, ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

type Props = Omit<ComponentProps<typeof Button>, "children"> & { label: string; children: ReactNode };

export default function IconButton({ label, children, ...props }: Props) {
  return <Tooltip><TooltipTrigger asChild><span className="inline-flex shrink-0"><Button variant="ghost" size="icon" aria-label={label} {...props}>{children}</Button></span></TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>;
}
