# Gum Python SDK

```python
import gum

@gum.job(retries=5, timeout="5m", rate_limit="20/m", concurrency=5)
def sync_customer(customer_id: str):
    ...

sync_customer.enqueue(customer_id="cus_123")
```
