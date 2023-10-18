import typing, os, time

from . import action, ffi
from .logger import logger

from . import action
from .ffi import bcolors


class ActionList:
    def __init__(self, actions: typing.List[action.Action]):
        self._actions = actions

    async def run(self, drop: ffi.Drop):
        logger.info("Processing action list...")
        for k, v in enumerate(self._actions):
            logger.info(
                f"{bcolors.HEADER}Running action {k+1}/{len(self._actions)}: {v}{bcolors.ENDC}"
            )
            start = time.time()

            await v.run(drop)

            elapsed = (time.time() - start) * 1000.0
            logger.info(
                f"{bcolors.HEADER}Action {k+1}/{len(self._actions)} completed in {elapsed} ms: {v}{bcolors.ENDC}"
            )

        logger.info("Done processing actions")


class Scenario:
    def __init__(
        self,
        id: str,
        desc: str,
        action_list: typing.Dict[str, ActionList],
        tags: typing.List[str] = [],
    ):
        self._id = id
        self._desc = desc
        self._action_list = action_list
        self._tags = tags

    def tags(self):
        return self._tags

    def desc(self):
        return self._desc

    def id(self):
        return self._id

    async def run(self, runner: str, drop: ffi.Drop):
        logger.info(f'Scenario ({self.id()}): "{self._desc}"')

        try:
            await self._action_list[runner].run(drop)
            os.seteuid(0)  # restore privileges, they might have been changed
        except Exception as e:
            logger.debug(f"Action threw an exception: {e}")
            raise

    def str(self):
        return f"Scenario({self._id}, {self._desc}, {self._action_list})"

    def runners(self):
        return self._action_list.keys()
