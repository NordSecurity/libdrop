import logging
from .colors import bcolors

logger = logging.getLogger("drop")

handler = logging.StreamHandler()
handler.setFormatter(
    logging.Formatter(
        bcolors.BOLD
        + "%(asctime)s:"
        + bcolors.ENDC
        + "%(name)s:%(levelname)s:%(message)s"
    )
)
logger.setLevel(logging.DEBUG)
logger.addHandler(handler)
