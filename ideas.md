# Feature Ideas

## Low-effort, high-value
- **Image upload** — already stubbed (`image: Option<String>` in the struct, just needs the endpoint + file input)
- **Filter markers by status** — hide Completed reports, or color-code markers by severity/status
- **Report clustering** — when zoomed out, group nearby markers with a count badge (Leaflet.markercluster)
- **Dark Mode Support**

## Public-facing UX
- **Upvote/confirm a report** — let others say "I see this too" to signal priority without submitting a duplicate
- **Share a report** — deep link to `/report/{id}` that opens the map centered on it
- **Status feed** — a simple chronological list of recently fixed reports ("Pothole on Main St fixed 2 days ago")

## Admin panel
- **Sort/filter the table** — by status, severity, or date
- **Export to CSV** — one button that downloads all reports
- **Bulk actions** — select multiple reports and mark them all In Progress / Completed at once
- **Notes field** — let admins leave internal notes on a report

## Longer-term
- **Email/SMS notifications** — notify submitters when their report's status changes (e.g. via Resend or Postmark)
- **Heatmap layer** — show where damage is concentrated using Leaflet.heat
- **Anonymous report IDs** — give submitters a code they can use to check their own report's status later
- **Moderation queue** — hold new submissions for admin review before they appear publicly (to combat spam)
