import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex items-center rounded-md border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-primary text-primary-foreground hover:bg-primary/80",
        secondary:
          "border-slate-200 bg-slate-500/10 text-slate-700 dark:border-slate-700 dark:bg-slate-500/10 dark:text-slate-400",
        success:
          "border-emerald-200 bg-emerald-500/15 text-emerald-700 dark:border-emerald-800 dark:bg-emerald-500/10 dark:text-emerald-400",
        info:
          "border-blue-200 bg-blue-500/15 text-blue-700 dark:border-blue-800 dark:bg-blue-500/10 dark:text-blue-400",
        "info-alternate":
          "border-violet-200 bg-violet-500/15 text-violet-700 dark:border-violet-800 dark:bg-violet-500/10 dark:text-violet-400",
        warning:
          "border-amber-200 bg-amber-500/15 text-amber-700 dark:border-amber-800 dark:bg-amber-500/10 dark:text-amber-400",
        error:
          "border-red-200 bg-red-500/15 text-red-700 dark:border-red-800 dark:bg-red-500/10 dark:text-red-400",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground hover:bg-destructive/80",
        outline: "text-foreground",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  )
}

export { Badge, badgeVariants }
