import { useEffect, useMemo, useState } from 'react';
import {
  clearUpcomingCache,
  fetchUpcomingReleases,
  releaseDateLabel,
  upcomingWindow,
  type MediaIntegration,
  type UpcomingRelease,
} from '../lib/integrations';
import type { AppSettings } from '../lib/settings';
import type { JellyfinSession } from '../lib/jellyfin';
import AppNav, { type AppView } from './AppNav';
import MaterialIcon from './MaterialIcon';

type Props = {
  session: JellyfinSession;
  settings: AppSettings;
  onNavigate: (view: AppView) => void;
  onSearch: () => void;
  onSignOut: () => void;
};

const weekDays = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const agendaDateFormatter = new Intl.DateTimeFormat(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
const syncTimeFormatter = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' });
type CalendarFilter = 'all' | 'radarr' | 'sonarr';

const filterOptions: Array<{ value: CalendarFilter; label: string }> = [
  { value: 'all', label: 'All' },
  { value: 'radarr', label: 'Movies' },
  { value: 'sonarr', label: 'Series' },
];

function startOfMonth(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function addDays(date: Date, days: number) {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

function dateKey(date: Date) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function eventDateKey(value: string) {
  return dateKey(new Date(value));
}

function monthGrid(month: Date) {
  const first = startOfMonth(month);
  const gridStart = addDays(first, -first.getDay());
  const days = Array.from({ length: 42 }, (_, index) => addDays(gridStart, index));
  return { days, start: gridStart, end: addDays(gridStart, 42) };
}

type CalendarGroup = { event: UpcomingRelease; count: number };

function groupDayEvents(events: UpcomingRelease[]): CalendarGroup[] {
  const groups = new Map<string, CalendarGroup>();
  for (const event of events) {
    const key = event.source === 'sonarr' && event.seriesId !== undefined
      ? `series-${event.seriesId}`
      : event.id;
    const existing = groups.get(key);
    if (existing) existing.count += 1;
    else groups.set(key, { event, count: 1 });
  }
  return [...groups.values()];
}

export default function Calendar({ session, settings, onNavigate, onSearch, onSignOut }: Props) {
  const [month, setMonth] = useState(() => startOfMonth(new Date()));
  const [events, setEvents] = useState<UpcomingRelease[]>([]);
  const [agendaEvents, setAgendaEvents] = useState<UpcomingRelease[]>([]);
  const [filter, setFilter] = useState<CalendarFilter>('all');
  const [loading, setLoading] = useState(true);
  const [errors, setErrors] = useState<Partial<Record<MediaIntegration, string>>>({});
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [selected, setSelected] = useState<UpcomingRelease | null>(null);
  const grid = useMemo(() => monthGrid(month), [month]);
  const start = grid.start.toISOString();
  const end = grid.end.toISOString();

  useEffect(() => {
    let cancelled = false;
    const window = upcomingWindow(new Date(), 120);
    fetchUpcomingReleases(settings, window.start, window.end).then((result) => {
      if (!cancelled) setAgendaEvents(result.events);
    });
    return () => { cancelled = true; };
  }, [refreshVersion, settings.radarrUrl, settings.sonarrUrl]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setErrors({});
    fetchUpcomingReleases(settings, start, end)
      .then((result) => {
        if (cancelled) return;
        setEvents(result.events);
        setErrors(result.errors);
        const configuredProviders = [settings.radarrUrl, settings.sonarrUrl].filter(Boolean).length;
        if (Object.keys(result.errors).length < configuredProviders) setLastUpdated(new Date());
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [end, refreshVersion, settings.radarrUrl, settings.sonarrUrl, start]);

  useEffect(() => {
    if (!selected) return;
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setSelected(null);
    };
    window.addEventListener('keydown', close);
    return () => window.removeEventListener('keydown', close);
  }, [selected]);

  const filteredEvents = useMemo(
    () => filter === 'all' ? events : events.filter((event) => event.source === filter),
    [events, filter],
  );
  const agendaItems = useMemo(
    () => (filter === 'all' ? agendaEvents : agendaEvents.filter((event) => event.source === filter)).slice(0, 5),
    [agendaEvents, filter],
  );

  const eventsByDay = useMemo(() => {
    const grouped = new Map<string, UpcomingRelease[]>();
    for (const event of filteredEvents) {
      const key = eventDateKey(event.date);
      const dayEvents = grouped.get(key) ?? [];
      dayEvents.push(event);
      grouped.set(key, dayEvents);
    }
    return grouped;
  }, [filteredEvents]);

  const monthLabel = new Intl.DateTimeFormat(undefined, { month: 'long', year: 'numeric' }).format(month);
  const todayKey = dateKey(new Date());
  const errorMessages = Object.values(errors).filter((message): message is string => Boolean(message));

  async function refreshCalendar() {
    await clearUpcomingCache();
    setRefreshVersion((current) => current + 1);
  }

  return (
    <main className="calendar-shell">
      <AppNav session={session} activeView="calendar" calendarEnabled onNavigate={onNavigate} onSearch={onSearch} onSignOut={onSignOut} />
      <header className="calendar-header">
        <div>
          <p className="eyebrow">Your requested releases</p>
          <h1>Coming Soon</h1>
          <p>Only monitored titles from your connected Radarr and Sonarr libraries.</p>
        </div>
        <div className="calendar-month-controls">
          <button type="button" aria-label="Previous month" onClick={() => setMonth((current) => new Date(current.getFullYear(), current.getMonth() - 1, 1))}>
            <MaterialIcon name="arrow_back" />
          </button>
          <button type="button" onClick={() => setMonth(startOfMonth(new Date()))}>Today</button>
          <button className="calendar-month-controls__next" type="button" aria-label="Next month" onClick={() => setMonth((current) => new Date(current.getFullYear(), current.getMonth() + 1, 1))}>
            <MaterialIcon name="arrow_back" />
          </button>
        </div>
      </header>

      <section className="calendar-content" aria-label={`${monthLabel} release calendar`}>
        <div className="calendar-sync-bar" aria-live="polite">
          <div className="calendar-integration-health">
            {settings.radarrUrl ? (
              <span className={errors.radarr ? 'calendar-health--error' : ''}>
                <i /> Radarr {errors.radarr ? 'unavailable' : loading && !lastUpdated ? 'connecting' : 'connected'}
              </span>
            ) : null}
            {settings.sonarrUrl ? (
              <span className={errors.sonarr ? 'calendar-health--error' : ''}>
                <i /> Sonarr {errors.sonarr ? 'unavailable' : loading && !lastUpdated ? 'connecting' : 'connected'}
              </span>
            ) : null}
          </div>
          <div className="calendar-sync-actions">
            <span>{lastUpdated ? `Updated ${syncTimeFormatter.format(lastUpdated)}` : 'Waiting for first update'}</span>
            <button type="button" disabled={loading} onClick={refreshCalendar}>{loading ? 'Refreshing…' : 'Refresh'}</button>
          </div>
        </div>
        <div className="calendar-agenda-heading">
          <div>
            <p className="eyebrow">Next up</p>
            <h2>Your upcoming premieres</h2>
          </div>
          <div className="calendar-filters" role="group" aria-label="Filter calendar by media type">
            {filterOptions.map((option) => (
              <button
                className={filter === option.value ? 'active' : ''}
                type="button"
                aria-pressed={filter === option.value}
                key={option.value}
                onClick={() => setFilter(option.value)}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
        {agendaItems.length ? (
          <div className="calendar-agenda" aria-label="Next five monitored releases">
            {agendaItems.map((event) => (
              <button className={`calendar-agenda-card calendar-agenda-card--${event.source}`} type="button" key={event.id} onClick={() => setSelected(event)}>
                {event.imageUrl ? <img src={event.imageUrl} alt="" /> : <span className="calendar-agenda-card__fallback"><MaterialIcon name={event.source === 'radarr' ? 'movie' : 'tv'} /></span>}
                <span className="calendar-agenda-card__copy">
                  <small>{releaseDateLabel(event.date)}</small>
                  <strong>{event.title}</strong>
                  <em>{event.releaseKind}{event.subtitle ? ` · ${event.subtitle}` : ''}</em>
                </span>
                <span className="calendar-agenda-card__date">{agendaDateFormatter.format(new Date(event.date))}</span>
              </button>
            ))}
          </div>
        ) : (
          <p className="calendar-agenda-empty">No upcoming {filter === 'all' ? 'releases' : filter === 'radarr' ? 'movies' : 'episodes'} in the next 120 days.</p>
        )}
        <div className="calendar-title-row">
          <h2>{monthLabel}</h2>
          <span>{filteredEvents.length} scheduled {filteredEvents.length === 1 ? 'release' : 'releases'}</span>
        </div>
        {errorMessages.length ? <div className="calendar-warning"><MaterialIcon name="info" /><span>{errorMessages.join(' ')}</span></div> : null}
        <div className="calendar-weekdays" aria-hidden="true">
          {weekDays.map((day) => <span key={day}>{day}</span>)}
        </div>
        <div className={`calendar-grid${loading ? ' calendar-grid--loading' : ''}`}>
          {grid.days.map((day) => {
            const key = dateKey(day);
            const groups = groupDayEvents(eventsByDay.get(key) ?? []);
            const outsideMonth = day.getMonth() !== month.getMonth();
            return (
              <article className={`calendar-day${outsideMonth ? ' calendar-day--outside' : ''}${key === todayKey ? ' calendar-day--today' : ''}`} key={key}>
                <span className="calendar-day__number">{day.getDate()}</span>
                <div className="calendar-day__events">
                  {groups.slice(0, 3).map(({ event, count }) => (
                    <button className={`calendar-event calendar-event--${event.source}`} type="button" key={event.id} onClick={() => setSelected(event)}>
                      {event.imageUrl ? <img src={event.imageUrl} alt="" /> : <MaterialIcon name={event.source === 'radarr' ? 'movie' : 'tv'} />}
                      <span><strong>{event.title}</strong><small>{count > 1 ? `${count} episodes` : event.releaseKind}</small></span>
                    </button>
                  ))}
                  {groups.length > 3 ? <span className="calendar-day__more">+{groups.length - 3} more</span> : null}
                </div>
              </article>
            );
          })}
        </div>
        {!loading && filteredEvents.length === 0 && errorMessages.length === 0 ? (
          <div className="calendar-empty"><strong>No monitored {filter === 'all' ? 'releases' : filter === 'radarr' ? 'movies' : 'episodes'} this month.</strong><span>Move to another month or update the filter.</span></div>
        ) : null}
      </section>

      {selected ? (
        <div className="upcoming-modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setSelected(null); }}>
          <section className="upcoming-modal" role="dialog" aria-modal="true" aria-labelledby="upcoming-modal-title">
            <button className="upcoming-modal__close" type="button" aria-label="Close release details" onClick={() => setSelected(null)}>×</button>
            {selected.imageUrl ? <img className="upcoming-modal__poster" src={selected.imageUrl} alt="" /> : null}
            <div className="upcoming-modal__copy">
              <p className="eyebrow">{selected.source === 'radarr' ? 'Radarr movie' : 'Sonarr episode'}</p>
              <h2 id="upcoming-modal-title">{selected.title}</h2>
              {selected.subtitle ? <p className="upcoming-modal__subtitle">{selected.subtitle}</p> : null}
              <p className="hero-meta">{releaseDateLabel(selected.date)} · {selected.releaseKind} · Monitored</p>
              {selected.milestones && selected.milestones.length > 1 ? (
                <div className="upcoming-modal__milestones" aria-label="Movie release milestones">
                  {selected.milestones.map((milestone) => (
                    <span className={milestone.kind === selected.releaseKind ? 'active' : ''} key={milestone.kind}>
                      <small>{milestone.kind}</small>
                      <strong>{agendaDateFormatter.format(new Date(milestone.date))}</strong>
                    </span>
                  ))}
                </div>
              ) : null}
              <p>{selected.overview || 'No overview is available from the connected service.'}</p>
              {selected.genres.length ? <div className="upcoming-modal__genres">{selected.genres.slice(0, 4).map((genre) => <span key={genre}>{genre}</span>)}</div> : null}
            </div>
          </section>
        </div>
      ) : null}
    </main>
  );
}
