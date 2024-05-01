from setuptools import setup, find_packages

setup(name='prometheus-fritzbox-exporter',
      version='0.1',
      # Modules to import from other scripts:
      packages=find_packages(),
      # Executables
      scripts=["fritzbox_exporter.py"],
     )

