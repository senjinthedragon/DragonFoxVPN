import requests
from bs4 import BeautifulSoup

url = "https://vpn.hatchling.org"
try:
    response = requests.get(url, verify=False)
    soup = BeautifulSoup(response.text, 'html.parser')
    
    print("--- RAW PARSE ---")
    for element in soup.select('.dropdown-content > *'):
        classes = element.get('class', [])
        text = element.get_text().strip()
        if 'optgroup-label' in classes:
            print(f"GROUP: {text}")
        elif 'dropdown-item' in classes:
            val = element.get('data-value')
            print(f"ITEM: [{val}] {text}")
except Exception as e:
    print(f"Error: {e}")
