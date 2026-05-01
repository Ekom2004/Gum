import os

import gum
import resend

resend.api_key = os.environ["RESEND_API_KEY"]

# For quick validation use every 5 minutes.
# Change to cron="0 9 * * 1" for Monday at 09:00 local business time.
@gum.job(cron="*/5 * * * *", timezone="America/New_York", retries=5, timeout="2m", concurrency=1)
def send_weekly_digest():
    resend.emails.send(
        from_=os.environ["RESEND_FROM"],
        to=os.environ["RESEND_TO"],
        subject="Gum scheduled email test",
        html="<p>This email was scheduled and sent by Gum.</p>",
    )
