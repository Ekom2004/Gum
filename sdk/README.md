# Gum Python SDK

```bash
pip install usegum
gum login
```

```python
import gum

openai_limit = gum.rate_limit("60/m")

@gum.job(retries=5, timeout="5m", cpu=2, memory="1gb", rate_limit=openai_limit, concurrency=5, key="customer_id")
def sync_customer(customer_id: str):
    ...

@gum.job(cron="0 9 * * 1", timezone="America/New_York")
def refresh_index():
    ...

sync_customer.enqueue(customer_id="cus_123")
sync_customer.enqueue(customer_id="cus_123", delay="10m")
```

```bash
gum deploy
gum get <run_id>
gum logs <run_id>
```

Optional operator mode (infrastructure only): auto-sync runner capacity during deploy.

```bash
export GUM_COMPUTE_PROVIDER=fly
export FLY_RUNNER_APP=gum-runner-stg
export GUM_RUNNER_PARALLELISM=1
gum deploy
```
