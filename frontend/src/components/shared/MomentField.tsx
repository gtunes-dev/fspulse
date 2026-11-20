import { useEffect, useRef, useState, type FocusEvent, type KeyboardEvent } from 'react'
import { format } from 'date-fns'
import { cn } from '@/lib/utils'

type Part = 'month' | 'day' | 'year' | 'hour'

const PARTS: Part[] = ['month', 'day', 'year', 'hour']
const PART_NAME: Record<Part, string> = { month: 'Month', day: 'Day', year: 'Year', hour: 'Hour' }

interface MomentFieldProps {
  /** The moment shown, Unix seconds. Only its month, day, year and hour are used. */
  value: number
  /** Called with every change, as it is made. The caller may adjust the moment; the field shows whatever `value` it is then given. */
  onChange: (seconds: number) => void
  /** Called when editing ends: focus leaves the field, or Enter is pressed. */
  onDone?: () => void
  /** What the field is, for assistive technology: "Start of the range". */
  label: string
  /** Put the keyboard focus on the hour when the field first appears. */
  focusOnMount?: boolean
}

function daysIn(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate()
}

/**
 * A date and hour that is edited in place, one part at a time: month,
 * day, year, hour. Click a part, then type it or use the up and down
 * arrow keys; left and right move between parts. There is no popup.
 *
 * The arrow keys carry: one hour past 23:00 is the next day, one day
 * past the end of a month is the next month.
 */
export function MomentField({ value, onChange, onDone, label, focusOnMount }: MomentFieldProps) {
  const date = new Date(value * 1000)
  // Digits typed into a part so far. A year is only used once all four are in.
  const [typed, setTyped] = useState<{ part: Part; text: string } | null>(null)
  const parts = useRef<Partial<Record<Part, HTMLSpanElement | null>>>({})

  useEffect(() => {
    if (focusOnMount) parts.current.hour?.focus()
    // Only when the field appears.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const emit = (next: Date) => onChange(Math.floor(next.getTime() / 1000))

  /** One more or one fewer of a part, carrying into the larger parts. */
  const step = (part: Part, by: number) => {
    if (part === 'hour' || part === 'day') {
      const next = new Date(date)
      if (part === 'hour') next.setHours(next.getHours() + by)
      else next.setDate(next.getDate() + by)
      emit(next)
      return
    }
    // A month or year keeps the day, or the last day there is: Jan 31 to Feb 28.
    const next = new Date(
      date.getFullYear() + (part === 'year' ? by : 0),
      date.getMonth() + (part === 'month' ? by : 0),
      1,
      date.getHours()
    )
    next.setDate(Math.min(date.getDate(), daysIn(next.getFullYear(), next.getMonth())))
    emit(next)
  }

  /** Set one part outright, leaving the others. */
  const set = (part: Part, amount: number) => {
    const year = part === 'year' ? amount : date.getFullYear()
    const month = part === 'month' ? amount - 1 : date.getMonth()
    const day = Math.min(part === 'day' ? amount : date.getDate(), daysIn(year, month))
    emit(new Date(year, month, day, part === 'hour' ? amount : date.getHours()))
  }

  const move = (part: Part, by: number) => {
    const target = PARTS[PARTS.indexOf(part) + by]
    if (target) parts.current[target]?.focus()
  }

  const typeDigit = (part: Part, digit: string) => {
    let text = (typed?.part === part ? typed.text : '') + digit
    if (part === 'year') {
      if (text.length < 4) {
        setTyped({ part, text })
        return
      }
      set('year', Math.max(1970, Number.parseInt(text, 10)))
      setTyped(null)
      move(part, 1)
      return
    }
    const least = part === 'hour' ? 0 : 1
    const most = part === 'hour' ? 23 : part === 'month' ? 12 : daysIn(date.getFullYear(), date.getMonth())
    // A second digit that makes too large a number starts the part again.
    if (Number.parseInt(text, 10) > most) text = digit
    const amount = Number.parseInt(text, 10)
    if (amount >= least && amount <= most) set(part, amount)
    // Finished when no further digit could follow.
    if (text.length >= 2 || amount * 10 > most) {
      setTyped(null)
      move(part, 1)
    } else {
      setTyped({ part, text })
    }
  }

  /** A letter in the month: the next month that starts with it. */
  const typeMonthLetter = (letter: string) => {
    for (let ahead = 1; ahead <= 12; ahead++) {
      const month = (date.getMonth() + ahead) % 12
      if (format(new Date(2000, month, 1), 'MMM').toLowerCase().startsWith(letter)) {
        set('month', month + 1)
        return
      }
    }
  }

  const onKeyDown = (part: Part, event: KeyboardEvent) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return
    const key = event.key
    if (key === 'ArrowUp' || key === 'ArrowDown') {
      setTyped(null)
      step(part, key === 'ArrowUp' ? 1 : -1)
    } else if (key === 'ArrowLeft' || key === 'ArrowRight') {
      setTyped(null)
      move(part, key === 'ArrowRight' ? 1 : -1)
    } else if (key === 'Enter') {
      ;(event.currentTarget as HTMLElement).blur()
    } else if (/^[0-9]$/.test(key)) {
      typeDigit(part, key)
    } else if (part === 'month' && /^[a-zA-Z]$/.test(key)) {
      typeMonthLetter(key.toLowerCase())
    } else {
      return
    }
    event.preventDefault()
  }

  const onBlur = (event: FocusEvent<HTMLSpanElement>) => {
    setTyped(null)
    // Moving between this field's own parts is not the end of editing.
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onDone?.()
  }

  const text: Record<Part, string> = {
    month: format(date, 'MMM'),
    day: format(date, 'd'),
    year: format(date, 'yyyy'),
    hour: format(date, 'HH'),
  }
  const amount: Record<Part, number> = {
    month: date.getMonth() + 1,
    day: date.getDate(),
    year: date.getFullYear(),
    hour: date.getHours(),
  }

  const part = (name: Part) => (
    <span
      ref={(element) => {
        parts.current[name] = element
      }}
      role="spinbutton"
      tabIndex={0}
      aria-label={`${label}: ${PART_NAME[name]}`}
      aria-valuenow={amount[name]}
      aria-valuetext={text[name]}
      className={cn(
        'rounded px-0.5 outline-none cursor-text transition-colors',
        'hover:bg-accent focus:bg-primary focus:text-primary-foreground'
      )}
      onKeyDown={(event) => onKeyDown(name, event)}
    >
      {typed?.part === name && name === 'year' ? typed.text.padEnd(4, '_') : text[name]}
    </span>
  )

  return (
    <span
      role="group"
      aria-label={label}
      title="Click a part, then type it or use the up and down arrow keys"
      className="inline-flex items-baseline rounded-md border border-input bg-background px-1.5 py-1 text-sm font-semibold tabular-nums transition-colors focus-within:border-primary"
      onBlur={onBlur}
    >
      {part('month')}
      <span className="w-0.5" />
      {part('day')}
      <span className="w-1.5" />
      {part('year')}
      <span className="w-2.5" />
      {part('hour')}
      <span className="text-muted-foreground">:00</span>
    </span>
  )
}
