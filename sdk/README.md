# Gum Python SDK

```python
import gum

openai_limit = gum.rate_limit("openai:60/m")

@gum.job(retries=5, timeout="5m", rate_limit=openai_limit, concurrency=5)
def sync_customer(customer_id: str):
    ...

sync_customer.enqueue(customer_id="cus_123")
```
