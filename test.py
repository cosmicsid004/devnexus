import os, numpy as np
import a.b.c
from requests import get
from . import sibling
from .pkg import thing

try:
    import ujson
except ImportError:
    import json

from typing import TYPE_CHECKING
if TYPE_CHECKING:
    import pandas

def f():
    import yaml