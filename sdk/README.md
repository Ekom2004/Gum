# Gum Python SDK

```bash
pip install usegum
```

```python
import gum

openai_limit = gum.rate_limit("60/m")

@gum.job(retries=5, timeout="5m", memory="1gb", rate_limit=openai_limit, concurrency=5, key="customer_id")
def sync_customer(customer_id: str):
    ...

sync_customer.enqueue(customer_id="cus_123")
```
