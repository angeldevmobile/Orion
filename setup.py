from setuptools import setup, find_packages
import os

def read(fname):
    return open(os.path.join(os.path.dirname(__file__), fname), encoding="utf-8").read()

setup(
    name="orion",
    version="1.1.0",
    author="Gabriel Zapata",
    description="Orion Language — Cognitive Runtime for next-gen programming.",
    long_description=read("README.md") if os.path.exists("README.md") else "",
    long_description_content_type="text/markdown",
    packages=find_packages(include=["orion", "orion.*"]),
    install_requires=[
        "rich>=13.0.0",
        "pyfiglet>=1.0.0",
        "textual>=0.60.0",
    ],
    entry_points={
        "console_scripts": [
            "orion=orion.cli:main",
        ],
    },
    include_package_data=True,
    python_requires=">=3.9",
)
