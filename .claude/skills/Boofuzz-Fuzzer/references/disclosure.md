# Vulnerability Disclosure and PoC Guidelines

## Responsible Disclosure Process

1. **Do not** open public issues for security vulnerabilities
2. Contact vendor security team directly
3. Allow 90 days for fix before public disclosure
4. Consider requesting CVE if appropriate

## Finding Vendor Contacts

Check in order:
- `SECURITY.md` in repository root
- GitHub Security Advisories (repository → Security tab)
- Project website security page
- Generic: `security@<vendor>.com` or `security@<vendor>.io`

## Disclosure Report Template

```
Subject: Security Vulnerability in [Product] - [Type]

## Summary
[One sentence description]

## Affected Versions
- Tested: [version]
- Likely affected: [range estimate]

## Vulnerability Details
- Type: [heap overflow / use-after-free / DoS / injection / etc.]
- Component: [parser / handler / etc.]
- Attack vector: Network / Local
- Authentication: Required / Not required

## Reproduction Steps
1. Start target with: [command]
2. Run fuzzer/PoC: [command]
3. Observe: [crash / ASAN output / behaviour]

## Proof of Concept
[Minimal reproducer - see PoC section below]

## Impact
[What can an attacker achieve? DoS, RCE, info leak?]

## Suggested Fix
[If obvious from analysis]

## Timeline
- [Date]: Discovered via fuzzing
- [Date]: Reported to vendor
- [Date]: Expected public disclosure (90 days)

## Credit
[Your name/handle if desired]
```

## Creating Minimal PoC Files

### From boofuzz Results

Extract the crashing test case from results database:

```python
import sqlite3

conn = sqlite3.connect('boofuzz-results/run-YYYY-MM-DD.db')
cursor = conn.cursor()

# Find crash case
cursor.execute("""
    SELECT id, name FROM cases 
    WHERE type LIKE '%fail%' LIMIT 1
""")
case_id, case_name = cursor.fetchone()

# Get the actual bytes sent
cursor.execute("""
    SELECT data FROM steps 
    WHERE case_id = ? AND type = 'send'
""", (case_id,))
crash_payload = cursor.fetchone()[0]

# Write raw payload
with open('crash_payload.bin', 'wb') as f:
    f.write(crash_payload)
```

### Standalone PoC Script

Create a minimal reproducer without boofuzz dependency:

```python
#!/usr/bin/env python3
"""
PoC for [CVE-XXXX-XXXXX]: [Vulnerability Type] in [Product]

Sends crafted [protocol] packet causing [effect].
Tested against [product] version [X.Y.Z].

Usage: python3 poc.py <target_host> [port]
"""
import socket
import sys

# Minimal payload that triggers the bug
PAYLOAD = bytes.fromhex(
    "10"           # Packet type
    "ff ff ff 7f"  # Malformed length (triggers overflow)
    "41" * 100     # Overflow data
)

def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <host> [port]")
        sys.exit(1)
    
    host = sys.argv[1]
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 1883
    
    print(f"[*] Connecting to {host}:{port}")
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(5)
    
    try:
        sock.connect((host, port))
        print(f"[*] Sending payload ({len(PAYLOAD)} bytes)")
        sock.send(PAYLOAD)
        
        # Try to receive response
        try:
            response = sock.recv(1024)
            print(f"[*] Received: {response.hex()}")
        except socket.timeout:
            print("[!] No response (possible crash)")
    except ConnectionRefusedError:
        print("[!] Connection refused (target may have crashed)")
    finally:
        sock.close()

if __name__ == "__main__":
    main()
```

### Including ASAN Output

If target was built with AddressSanitizer, include relevant output:

```
=================================================================
==12345==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x...
READ of size 4 at 0x... thread T0
    #0 0x... in vulnerable_function src/parser.c:123
    #1 0x... in handle_packet src/handler.c:456
    #2 0x... in main src/main.c:78

0x... is located 0 bytes to the right of 100-byte region [...]
allocated by thread T0 here:
    #0 0x... in malloc
    #1 0x... in allocate_buffer src/parser.c:100
```

## PoC File Naming

```
CVE-XXXX-XXXXX_product_vuln-type.py
poc_product_heap-overflow.py
crash_mqtt_connect_malformed-length.bin
```

## PoC Best Practices

- Include clear header comment with CVE, product, version
- Minimal dependencies (prefer stdlib only)
- Single file when possible
- Include usage instructions
- Note tested versions/environments
- Do not include weaponised exploits (DoS PoC is sufficient)

## Sharing PoC After Disclosure

After coordinated disclosure:
- GitHub Gist or dedicated security advisory repo
- Reference in CVE entry
- Consider Exploit-DB for archival
